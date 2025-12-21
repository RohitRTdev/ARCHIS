use core::alloc::Layout;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::ptr::NonNull;
use common::{PAGE_SIZE, align_up};
use kernel_intf::{KError, info};
use crate::hal::get_core;
use crate::infra::disable_early_panic_phase;
use crate::{ds::*, hal};
use crate::sync::Spinlock;
use crate::mem::{PageDescriptor, Regions::*, allocate_memory, deallocate_memory, map_memory, unmap_memory};

pub const INIT_STACK_SIZE: usize  = PAGE_SIZE * 3;
pub const INIT_GUARD_PAGE_SIZE: usize = PAGE_SIZE;
pub const TOTAL_STACK_SIZE: usize = INIT_STACK_SIZE + INIT_GUARD_PAGE_SIZE;

#[cfg_attr(target_arch = "x86_64", repr(align(4096)))]
struct KStack {
    stack: [u8; PAGE_SIZE]
}

static KERN_BACKUP_STACK: KStack = KStack {
    stack: [0; PAGE_SIZE]
};

pub struct Stack {
    guard_size: usize,
    stack_size: usize,
    base: NonNull<u8>
}

impl Stack {
    // Create STACK + GUARD page. The guard page will remain unmapped
    // This is to allow us to catch any stack overflow scenarios
    pub fn new() -> Result<Self, KError> {
        Self::new_with(INIT_STACK_SIZE, INIT_GUARD_PAGE_SIZE)
    }
    
    pub fn new_with(stack_size: usize, guard_size: usize) -> Result<Self, KError> {
        // TODO: Roll back and deallocate any memory allocations made in case operations further down fail
        let stack_raw = allocate_memory(Layout::from_size_align(stack_size + guard_size, PAGE_SIZE).unwrap()
        , PageDescriptor::VIRTUAL | PageDescriptor::NO_ALLOC)?;

        let stack_raw_phys = allocate_memory(Layout::from_size_align(stack_size, PAGE_SIZE).unwrap(),
    0)?;

        #[cfg(feature = "stack_down")]
        let stack_base = unsafe {
            stack_raw.add(guard_size)
        };

        #[cfg(not(feature = "stack_down"))]
        let stack_base = stack_raw;

        map_memory(stack_raw_phys.addr(), stack_base.addr(), stack_size, PageDescriptor::VIRTUAL)?;

        Ok(Self {guard_size: guard_size, stack_size: stack_size, base: NonNull::new(stack_raw).unwrap() })
    }

    pub fn destroy(&mut self) {
        unmap_memory(self.get_stack_top() as *mut u8, self.stack_size).expect("Stack base address wrong during unmap??");
        
        // Deallocate the guard page memory (if any)
        if self.guard_size != 0 {
            deallocate_memory(
                self.get_alloc_base() as *mut u8,
                Layout::from_size_align(self.guard_size, PAGE_SIZE).unwrap()
            , PageDescriptor::VIRTUAL)
            .expect("Failed to deallocate memory for stack")

        }
    }
    
    #[cfg(feature = "stack_down")]
    #[inline(always)]
    pub fn get_alloc_base(&self) -> usize {
        self.base.as_ptr().addr()
    }
    
    #[cfg(not(feature = "stack_down"))]
    #[inline(always)]
    pub fn get_alloc_base(&self) -> usize {
        self.base.as_ptr().addr() + self.stack_size 
    }
    
    #[cfg(feature = "stack_down")]
    #[inline(always)]
    pub fn get_stack_base(&self) -> usize {
        self.base.as_ptr().addr() + self.guard_size + self.stack_size
    }

    #[cfg(not(feature = "stack_down"))]
    #[inline(always)]
    pub fn get_stack_base(&self) -> usize {
        self.base.as_ptr().addr()
    }
    
    #[cfg(feature = "stack_down")]
    #[inline(always)]
    pub fn get_stack_top(&self) -> usize {
        self.base.as_ptr().addr() + self.guard_size
    }

    #[cfg(not(feature = "stack_down"))]
    #[inline(always)]
    pub fn get_stack_top(&self) -> usize {
        self.base.as_ptr().addr() + self.stack_size
    }
}

struct CPUControlBlock {
    id: usize,
    worker_stack: Stack,
    good_stack: Stack,
    panic_base: usize
}

unsafe impl Send for CPUControlBlock {}

pub const MAX_CPUS: usize = 64; 

static CPU_ID: AtomicUsize = AtomicUsize::new(0);
static CPU_LIST: Spinlock<FixedList<CPUControlBlock, {Region4 as usize}>> = Spinlock::new(List::new());

pub fn init() {
    register_cpu();
}

pub fn register_cpu() -> usize {
    let cpu_id = CPU_ID.fetch_add(1, Ordering::Relaxed);
    let cb = if cpu_id == 0 {
        // This is prematurely created just to take care of early panic management
        CPUControlBlock {
            id: cpu_id,
            // We will not make use of this at this stage. This value is given just to initialize it
            worker_stack: Stack {
                stack_size: INIT_STACK_SIZE,
                guard_size: INIT_GUARD_PAGE_SIZE,
                base: NonNull::new(align_up(hal::get_current_stack_base(), PAGE_SIZE) as *mut u8).unwrap()
            },
            good_stack: Stack {
                stack_size: PAGE_SIZE,
                guard_size: 0,
                base: NonNull::new(KERN_BACKUP_STACK.stack.as_ptr() as *mut u8).unwrap()
            },
            panic_base: hal::get_current_stack_base()
        }
    } else {
        // Allocate worker stack for the CPU
        let stack = Stack::new().expect("Failed to allocate memory for CPU worker stack");
        let backup_stack = Stack::new_with(PAGE_SIZE, 0).expect("Failed to create backup stack for cpu");
        let stack_base = stack.get_stack_base();

        CPUControlBlock {
            id: cpu_id,
            worker_stack: stack,
            good_stack: backup_stack,
            panic_base: stack_base
        }
    };

    info!("Registered CPU with core_id:{} and stack:{:#X}", cpu_id, cb.worker_stack.get_stack_top());

    CPU_LIST.lock().add_node(cb).expect("Failed to add CPU control block to the list");

    disable_early_panic_phase();
    cpu_id
}


// This should be called once memory manager is up
pub fn set_worker_stack_for_boot_cpu(stack_base: *mut u8) {
    let stack = Stack {stack_size: INIT_STACK_SIZE, guard_size: INIT_GUARD_PAGE_SIZE, 
        base: NonNull::new(stack_base).unwrap()};

    let core_id = hal::get_core();
    let mut cpu_list = CPU_LIST.lock();

    cpu_list.iter_mut().find(|cb| cb.id == core_id)
        .expect("Could not find cpu descriptor for boot cpu")
        .worker_stack = stack;
}

pub fn get_current_stack_base() -> usize {
    let core_id = hal::get_core();
    let cpu_list = CPU_LIST.lock();

    cpu_list.iter().find(|cb| cb.id == core_id)
        .and_then(|cb| Some(cb.worker_stack.get_stack_base()))
        .expect("Current core does not have associated cpu descriptor!")
}

pub fn get_current_good_stack_base() -> usize {
    let core_id = hal::get_core();
    let cpu_list = CPU_LIST.lock();

    cpu_list.iter().find(|cb| cb.id == core_id)
        .and_then(|cb| Some(cb.good_stack.get_stack_base()))
        .expect("Current core does not have associated cpu descriptor!")
}

pub fn get_panic_base() -> usize {
    let core_id = hal::get_core();
    let cpu_list = CPU_LIST.lock();

    cpu_list.iter().find(|cb| cb.id == core_id)
        .and_then(|cb| Some(cb.panic_base))
        .expect("Invalid core ID!!")
}

pub fn set_panic_base(base: usize) {
    let core_id = hal::get_core();
    let mut cpu_list = CPU_LIST.lock();

    if let Some(cb) = cpu_list.iter_mut().find(|cb| cb.id == core_id) {
        cb.panic_base = base;
    } else {
        panic!("Unable to find CPU control block for core_id:{}", core_id);
    }
}

// Usual cacheline size
#[repr(align(64))]
pub struct PerCpu<T: Sync> {
    pub data: [T; MAX_CPUS],
}

unsafe impl<T: Sync> Sync for PerCpu<T> {}

impl<T: Copy + Sync> PerCpu<T> {
    pub const fn new(init: T) -> Self {
        Self {
            data: [init; MAX_CPUS],
        }
    }
}

impl<T: Sync> PerCpu<T> {
    pub const fn new_with(init: [T; MAX_CPUS]) -> Self {
        Self { data: init }
    }
}

impl<T: Sync> PerCpu<T> {
    #[inline(always)]
    pub fn local(&self) -> &T {
        let cpu = get_core();
        &self.data[cpu]
    }

    // Caller must ensure correctness.
    #[inline(always)]
    pub unsafe fn get(&self, cpu: usize) -> &T {
        &self.data[cpu]
    }
}