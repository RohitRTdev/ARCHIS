use core::alloc::Layout;
use core::marker::PhantomData;
use core::mem;
use core::ptr::NonNull;
use common::ceil_div;
use crate::sync::Spinlock;

#[repr(usize)]
pub enum Regions {
    Region0,
    Region1
}

const BOOT_REGION_SIZE0: usize = 10 * 4096;
const BOOT_REGION_SIZE1: usize = 4096;
const TOTAL_BOOT_MEMORY: usize = (BOOT_REGION_SIZE0 + BOOT_REGION_SIZE1);

// Here we simply divide given memory into slots each of size 8 bytes
// 8 is chosen to represent an average DS size
const MIN_SLOT_SIZE: usize = 8;
const BITMAP_SIZE: usize = (TOTAL_BOOT_MEMORY / MIN_SLOT_SIZE) >> 3;

// Wrapper required to force alignment constraint
#[repr(align(4096))]
struct HeapWrapper {
    heap0: [u8; BOOT_REGION_SIZE0],
    heap1: [u8; BOOT_REGION_SIZE1],
    bitmap: [u8; BITMAP_SIZE],
    lock: Spinlock<core::marker::PhantomData<bool>>
}

static HEAP: HeapWrapper = HeapWrapper { 
    heap0: [0; BOOT_REGION_SIZE0],
    heap1: [0; BOOT_REGION_SIZE1],
    bitmap: [0; BITMAP_SIZE],
    lock: Spinlock::new(core::marker::PhantomData)
};

#[cfg(test)]
pub fn get_heap(reg: Regions) -> (*const u8, *const u8) {
    let _guard = HEAP.lock.lock();
    let (heap, bitmap_offset) = match reg {
        Regions::Region0 => {
            (HEAP.heap0.as_ptr() as *mut u8, 0)
        }
        Regions::Region1 => {
            (HEAP.heap1.as_ptr() as *mut u8, BOOT_REGION_SIZE0 >> 3)
        }
    };
    
    let bm = unsafe {
        HEAP.bitmap.as_ptr().add(bitmap_offset)
    };

    (heap, bm)
}

#[cfg(test)]
pub fn clear_heap() {
    let _guard = HEAP.lock.lock();
    unsafe {
        (&HEAP.bitmap as *const u8 as *mut u8).write_bytes(0, BITMAP_SIZE);
    }
}


// Forces FixedAllocator monomorphization only when slot size (size of the contained data) is >= MIN_SLOT_SIZE
pub struct FixedAllocator<T, const REGION: usize> 
where [(); mem::size_of::<T>() - MIN_SLOT_SIZE]: {
    _marker: PhantomData<T> 
}

impl<T, const REGION: usize> FixedAllocator<T, REGION> 
where [(); mem::size_of::<T>() - MIN_SLOT_SIZE]: {
    fn fetch_hdr_and_base() -> (*mut u8, *mut u8) {
        // We can safely borrow heap as mutable, since we're ensuring synchronization with lock
        let (heap_base, bitmap_offset) = match REGION {
            0 => {
                (HEAP.heap0.as_ptr() as *mut u8, 0)
            }
            1 => {
                (HEAP.heap1.as_ptr() as *mut u8, BOOT_REGION_SIZE0 >> 3)
            }

            // This will never happen
            _ => {(core::ptr::null_mut(),0)}
        };
        
        let bitmap_base = unsafe {
            HEAP.bitmap.as_ptr().add(bitmap_offset)
            as *mut u8
        };

        (heap_base, bitmap_base)
    }

    fn calculate_total_slots() -> usize {
        match REGION {
            0 => {
                BOOT_REGION_SIZE0 >> 3
            }
            1 => {
                BOOT_REGION_SIZE1 >> 3
            }
            _ => {
                0
            }
        }
     }
}


impl<T, const REGION: usize> super::Allocator<T> 
for FixedAllocator<T, REGION> 
where [(); mem::size_of::<T>() - MIN_SLOT_SIZE]: {

    fn alloc(layout: Layout) -> NonNull<T> {
        let _sync = HEAP.lock.lock();
        
        let (base, hdr_base) = Self::fetch_hdr_and_base();
        let slot_size = mem::size_of::<T>();
        let num_slots = Self::calculate_total_slots(); 
        let mut slots_required = ceil_div(layout.size(), slot_size);
        let mut slot_offset= 0;
        let mut start_slot = 0;
        let mut num_slots_found = 0;

        for slot_idx in 0..BITMAP_SIZE {
            // Search bitmap to find 'n' continuous free slots
            let slot_group = unsafe {
                *hdr_base.add(slot_idx)
            };
            for bit in 0..8 {
                if slot_group & (1 << bit) == 0 {
                    if num_slots_found == 0 {
                        start_slot = slot_offset;
                    }
                    
                    num_slots_found += 1;

                    if num_slots_found == slots_required {
                        break;
                    }
                }
                else {
                    num_slots_found = 0;
                }
                slot_offset += 1;
            }
            
            if num_slots_found == slots_required {
                break;
            }
        }

        let sel_slot = start_slot;
        if slot_offset >= num_slots {
            panic!("Fixed allocator region:{} ran out of space, num_slots:{}, slots_required:{}, num_slots_found:{}!", 
            REGION, num_slots, slots_required, num_slots_found);
        }

        // Set all those n bits to '1'
        while slots_required > 0 {
            let slot_idx = start_slot >> 3;
            let bit_idx = start_slot % 8;
            let mut slot_group = unsafe {
                *hdr_base.add(slot_idx)
            };

            slot_group |= 1 << bit_idx;
            unsafe {
                *hdr_base.add(slot_idx) = slot_group;
            }

            start_slot += 1;
            slots_required -= 1;
        }

        unsafe {
            NonNull::new(base.add(sel_slot * slot_size) as *mut T).unwrap()
        }
    }

    unsafe fn dealloc(address: NonNull<T>, layout: Layout) {
        let _sync = HEAP.lock.lock();
        
        let (base, hdr_base) = Self::fetch_hdr_and_base();

        let total_size = layout.size();
        let slot_size = mem::size_of::<T>();
        let mut slots = ceil_div(total_size, slot_size);
        let mut slot_offset = (address.as_ptr() as usize - base as usize) / slot_size;
        let num_slots = Self::calculate_total_slots(); 

        debug_assert!(slot_offset < num_slots, 
            "Wrong address given to dealloc function for fixed allocator => slot_offset:{}, num_slots:{} for Fixed allocator Region:{}!", 
            slot_offset, num_slots, REGION);

        while slots > 0 {
            let slot_group = slot_offset >> 3;
            let bit_idx = slot_offset % 8;
            
            unsafe {
                // Clear that bit in the given byte (0 means free)
                let slot = *hdr_base.add(slot_group);
                *hdr_base.add(slot_group) = slot & !(1 << bit_idx);  
            } 
            
            slot_offset += 1;
            slots -= 1;
        }

    }
}