use alloc::sync::{Arc, Weak};
use alloc::collections::BTreeMap;
use kernel_intf::KError;
use crate::{ds::*, sched};
use crate::mem::{self, PoolAllocatorGlobal, VCB, VirtMemConBlk};
use crate::sched::*;
use crate::sync::Spinlock;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::ptr::NonNull;
use core::mem::take;
use kernel_intf::info;

static PROCESS_ID: AtomicUsize = AtomicUsize::new(0);
static PROCESSES: Spinlock<BTreeMap<usize, KProcess>> = Spinlock::new(BTreeMap::new());

pub type KProcess = Arc<Spinlock<Process>, PoolAllocatorGlobal>;
pub type KThreadWeak = Weak<Spinlock<Task>, PoolAllocatorGlobal>;

#[derive(Clone, Copy, PartialEq)]
pub enum ProcessStatus {
    Ready,
    Terminated
}

pub struct Process {
    id: usize,
    // In current design, we will have the process struct holding weak pointers to the tasks.
    // Intuitively it should be the other way around, however this way it makes it easier code wise.
    // When tasks are dropped, the process struct will be automatically dropped
    threads: DynList<usize>,
    addr_space: VCB,
    status: ProcessStatus
}

unsafe impl Send for Process {}

impl Process {
    fn new(clone_addr_space: bool) -> Result<KProcess, KError> {
        let id = PROCESS_ID.fetch_add(1, Ordering::Relaxed);  
        let kernel_addr_space = mem::get_kernel_addr_space();
        let new_addr_space = if clone_addr_space {
            VirtMemConBlk::clone(kernel_addr_space, id)?     
        }
        else {
            kernel_addr_space
        };

        let proc = Arc::new_in(Spinlock::new(Self {
            id,
            threads: List::new(),
            addr_space: new_addr_space,
            status: ProcessStatus::Ready 
        }), PoolAllocatorGlobal);
        
        info!("Creating new process with id {}", id);

        Ok(proc)
    }

    pub fn get_vcb(&self) -> VCB {
        self.addr_space
    }

    pub fn get_status(&self) -> ProcessStatus {
        self.status
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn attach_thread_to_current_process(&mut self, thread_id: usize) -> Result<(), KError> {
        self.threads.add_node(thread_id)
    }

    pub fn remove_thread(&mut self, thread_id: usize) {
        if self.status == ProcessStatus::Terminated {
            return;
        }

        let mut killed_thread = None;
        for node in self.threads.iter() {
            if **node == thread_id {
                killed_thread = Some(NonNull::from(node));
                break;
            }
        }

        info!("Remove thread called with id {}", thread_id);

        unsafe {
            if let Some(killed_thread) = killed_thread {
                self.threads.remove_node(killed_thread);
            }
        }

        if self.threads.get_nodes() == 0 {
            self.destroy_process();
        }
    }

    fn destroy_process(&mut self) {
        self.status = ProcessStatus::Terminated;
        PROCESSES.lock().remove(&self.id);
        kernel_intf::debug!("Called destroy process {}", self.id);
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        info!("Dropping process {}", self.id);
    }
}

pub fn init() {
    // Create init process and attach init task (task id = 0) to it
    let init_proc = Process::new(false)
    .expect("Failed to create init process");

    PROCESSES.lock().insert(0, Arc::clone(&init_proc));

    let mut proc = init_proc.lock();
    proc.status = ProcessStatus::Ready;
    proc.threads.add_node(0).expect("Init process allocation failed!");

    info!("Created init process 0");
}

pub fn get_current_process() -> Option<KProcess> {
    let task = get_current_task()?;
    let guard = task.lock();
    guard.get_process()
}

pub fn get_current_process_id() -> Option<usize> {
    Some(get_current_process()?.lock().get_id())
}

pub fn get_process_info(proc_id: usize) -> Option<KProcess> {
    let proc_map = PROCESSES.lock();

    proc_map.get(&proc_id).map(|item| {
        Arc::clone(item)
    })
}

pub fn create_process(start_function: fn() -> !) -> Result<KProcess, KError> {
    disable_preemption();
    let process = Process::new(true)?;
    
    let thread = sched::create_init_thread(start_function, Arc::clone(&process))?;
    let core = thread.lock().get_core();

    start_task(&thread, core, &process, &PROCESSES)?;

    enable_preemption();    
    Ok(process)
}

pub fn kill_process(proc_id: usize) {
    let proc = get_process_info(proc_id);
    if proc.is_none() {
        return;
    }

    assert!(proc_id != 0, "Attempted to kill system process!");

    let proc = proc.unwrap();
    let cur_task_id = get_current_task_id();

    disable_preemption();

    // We manually move the list here since we don't want to hold the lock
    let threads = {
        let mut guard = proc.lock();
        if guard.status != ProcessStatus::Ready {
            return;
        }

        guard.status = ProcessStatus::Terminated;

        take(&mut guard.threads)
    };

    kernel_intf::debug!("Killing process {}", proc_id);

    let is_idle_task = cur_task_id.is_none();
    let cur_task_id = if cur_task_id.is_some() {cur_task_id.unwrap()} else {0};
    let mut is_exit = false; 

    // Kill all the tasks within the process
    for thread_id in threads.iter() {
        // We don't want the current task to kill itself right away
        // This happens if the current process is killing itself (exit)
        if is_idle_task || **thread_id != cur_task_id {
            kernel_intf::debug!("Issuing kill to thread {}", **thread_id);
            sched::kill_thread(**thread_id);
        }
        else {
            is_exit = true;
        }
    }
    
    proc.lock().destroy_process();

    
    enable_preemption();

    // Kill the current thread last
    if is_exit {
        // Drop it explicitly since we are not going to return from this call
        drop(proc);
        sched::kill_thread(cur_task_id);
    }
}

pub fn exit_process() -> ! {
    let proc_id = get_current_process_id().expect("Attempted to kill idle process!!");

    kill_process(proc_id);

    panic!("exit_process unreachable reached!!");
}