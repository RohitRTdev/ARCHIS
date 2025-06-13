use core::alloc::Layout;
use core::marker::PhantomData;
use core::mem;
use core::ptr::NonNull;
use common::ceil_div;
use crate::sync::Spinlock;

#[repr(usize)]
pub enum Regions {
    Region0,
    Region1,
    Region2,
    Region3,
    NumRegions 
}

const BOOT_REGION_SIZE: usize = 4096;
const TOTAL_BOOT_MEMORY: usize = BOOT_REGION_SIZE * Regions::NumRegions as usize;

// Here we simply divide given memory into slots each of size 8 bytes
// 8 is chosen to represent an average DS size
const MIN_SLOT_SIZE: usize = 8;
const BITMAP_SIZE: usize = (TOTAL_BOOT_MEMORY / MIN_SLOT_SIZE) >> 3;
const TOTAL_BITMAP_SIZE: usize = BITMAP_SIZE * Regions::NumRegions as usize;

// Wrapper required to force alignment constraint
#[repr(align(4096))]
struct HeapWrapper {
    heap: [u8; TOTAL_BOOT_MEMORY],
    bitmap: [u8; TOTAL_BITMAP_SIZE],
    lock: Spinlock<core::marker::PhantomData<bool>>
}

static HEAP: HeapWrapper = HeapWrapper { 
    heap: [0; TOTAL_BOOT_MEMORY],
    bitmap: [0; TOTAL_BITMAP_SIZE],
    lock: Spinlock::new(core::marker::PhantomData)
};

#[cfg(test)]
pub fn get_heap(reg: Regions) -> (*const u8, *const u8) {
    let _guard = HEAP.lock.lock();
    let region = reg as usize;
    let heap = unsafe {
        HEAP.heap.as_ptr().add(region * BOOT_REGION_SIZE)
    };  
    let r0_bm = unsafe {
        HEAP.bitmap.as_ptr().add(region * BITMAP_SIZE)
    };

    (heap, r0_bm)
}

#[cfg(test)]
pub fn clear_heap() {
    let _guard = HEAP.lock.lock();
    unsafe {
        (&HEAP.bitmap as *const u8 as *mut u8).write_bytes(0, TOTAL_BITMAP_SIZE);
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
        let base = unsafe {
            HEAP.heap.as_ptr().add(REGION * BOOT_REGION_SIZE)
            as *mut u8
        };
        
        let hdr_base = unsafe {
            HEAP.bitmap.as_ptr().add(REGION * BITMAP_SIZE)
            as *mut u8
        };

        (base, hdr_base)
    }
}


impl<T, const REGION: usize> super::Allocator<T> 
for FixedAllocator<T, REGION> 
where [(); mem::size_of::<T>() - MIN_SLOT_SIZE]: {

    fn alloc(layout: Layout) -> NonNull<T> {
        let _sync = HEAP.lock.lock();
        
        let (base, hdr_base) = Self::fetch_hdr_and_base();
        let slot_size = mem::size_of::<T>();
        let num_slots = BOOT_REGION_SIZE / slot_size;
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
        let num_slots = BOOT_REGION_SIZE / slot_size;

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