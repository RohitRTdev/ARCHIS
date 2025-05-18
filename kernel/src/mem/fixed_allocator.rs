use core::ffi::c_void;
use core::alloc::Layout;
use core::marker::PhantomData;
use core::mem;

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

// Wrapper required to force alignment constraint
#[repr(align(4096))]
struct HeapWrapper {
    heap: [u8; TOTAL_BOOT_MEMORY],
    bitmap: [u8; BITMAP_SIZE]
}

static HEAP: HeapWrapper = HeapWrapper { 
    heap: [0; TOTAL_BOOT_MEMORY],
    bitmap: [0; BITMAP_SIZE]
};

// Forces FixedAllocator monomorphization only when slot size (size of the contained data) is >= MIN_SLOT_SIZE
struct FixedAllocator<T, const REGION: usize> 
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


impl<T, const REGION: usize> super::Allocator 
for FixedAllocator<T, REGION> 
where [(); mem::size_of::<T>() - MIN_SLOT_SIZE]: {

    fn alloc(layout: Layout) -> *mut c_void {
        let (base, hdr_base) = Self::fetch_hdr_and_base();
        let num_slots = BOOT_REGION_SIZE / layout.size();
        let mut slot_offset= 0;

        for slot_idx in 0..BITMAP_SIZE {
            let mut slot_group = unsafe {
                *hdr_base.add(slot_idx)
            };
            for bit in 0..8 {
                if slot_group & (1 << bit) == 0 {
                    slot_group |= 1 << bit;
                    unsafe {
                        *(hdr_base.add(slot_idx) as *mut u8) = slot_group;
                    }
                    break;
                }
                slot_offset += 1;
            }
        }

        unsafe {
            base.add(slot_offset * layout.size())
            as *mut c_void
        }
    }

    fn dealloc(address: *mut c_void) {
        let (base, hdr_base) = Self::fetch_hdr_and_base();

        let slot_size = mem::size_of::<T>();
        let slot_offset = (address as usize - base as usize) / slot_size;
        let slot_group = slot_offset >> 3;
        let group_idx = slot_offset % 8;

        unsafe {
            let slot = *hdr_base.add(slot_group);
            *(hdr_base.add(slot_group) as *mut u8) = slot & !(1 << group_idx);  
        } 
    }
}