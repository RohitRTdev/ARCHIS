use alloc::sync::Arc;
use crate::cpu::{MAX_CPUS, PerCpu, Stack, get_panic_base, set_panic_base, get_total_cores, get_worker_stack};
use crate::hal::{self, IPIRequestType, create_kernel_context, disable_scheduler_timer, enable_scheduler_timer, fetch_context, switch_context};
use crate::mem::PoolAllocatorGlobal;
use crate::ds::*;
use crate::sync::{KSem, KSemInnerType, Spinlock};
use core::sync::atomic::{AtomicU8,AtomicUsize, Ordering};
use core::ptr::NonNull;
use alloc::collections::BTreeMap;
use kernel_intf::{KError, debug, info};

// This is in milliseconds
pub const QUANTUM: usize = 5;
const INIT_QUANTA: usize = 2;

pub type KThread = Arc<Spinlock<Task>, PoolAllocatorGlobal>;

static TASK_ID: AtomicUsize = AtomicUsize::new(0);
static TASK_CPU: AtomicU8 = AtomicU8::new(0);
static TASKS: Spinlock<BTreeMap<usize, KThread>> = Spinlock::new(BTreeMap::new());

const _: () = {
    assert!(u8::MAX as usize + 1 >= MAX_CPUS);
};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TaskStatus {
    RUNNING,
    ACTIVE,
    WAITING,
    TERMINATED
}

pub struct Task {
    id: usize,
    core: usize,
    stack: Option<Stack>,
    status: TaskStatus,
    context: *const u8,
    quanta: usize,
    panic_base: usize,
    wait_semaphores: DynList<KSemInnerType>,
    term_notify: KSem
}

impl Task {
    fn new(alloc_stack: bool, core: usize) -> Result<KThread, KError> {
        let stack  = if alloc_stack {
            Some(Stack::new()?)
        } else {
            None
        };
        let id = TASK_ID.fetch_add(1, Ordering::Relaxed);  

        if alloc_stack {
            debug!("Creating task with ID:{} and stack_addr={:#X}", id, stack.as_ref().unwrap().get_stack_base());
        } 
        else {
            debug!("Creating task with ID:{}", id);
        }

        let task = Arc::new_in(Spinlock::new(Task {
            id,
            core, 
            stack,
            status: TaskStatus::ACTIVE,
            context: core::ptr::null(),
            quanta: INIT_QUANTA,
            panic_base: 0,
            wait_semaphores: List::new(),
            term_notify: KSem::new(0, 1)
        }), PoolAllocatorGlobal);

        TASKS.lock().insert(id, Arc::clone(&task));

        Ok(task)
    }

    pub fn get_id(&self) -> usize {
        self.id
    }
    
    pub fn get_status(&self) -> TaskStatus {
        self.status
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        info!("Dropping task:{}", self.id);
        assert!(self.wait_semaphores.get_nodes() == 0);
    }
}

unsafe impl Send for Task {}

pub struct TaskQueue {
    active_tasks: DynList<KThread>,
    waiting_tasks: DynList<KThread>,
    terminated_tasks: DynList<KThread>,
    running_task: Option<NonNull<ListNode<KThread>>>,
    idle_task_stack: NonNull<u8> 
}

unsafe impl Send for TaskQueue{}

impl TaskQueue {
    const fn new() -> Self {
        TaskQueue {
            active_tasks: List::new(),
            waiting_tasks: List::new(),
            terminated_tasks: List::new(),
            running_task: None,
            idle_task_stack: NonNull::dangling()
        }
    }
}

static SCHEDULER_CON_BLK: PerCpu<Spinlock<TaskQueue>> = PerCpu::new_with(
    [const {Spinlock::new(TaskQueue::new())}; MAX_CPUS]
);

pub fn get_task_info(task_id: usize) -> Option<KThread> {
    let task_map = TASKS.lock();

    task_map.get(&task_id).map(|item| {
        Arc::clone(item)
    })
}

// None indicates that idle task is currently running
pub fn get_current_task() -> Option<KThread> {
    let cb = SCHEDULER_CON_BLK.local().lock().running_task;

    if cb.is_none() {
        return None;
    }

    Some(Arc::clone(unsafe {
        &**cb.unwrap().as_ptr()
    }))
}

pub fn get_num_active_tasks() -> usize {
    let sched_cb = SCHEDULER_CON_BLK.local().lock();
    sched_cb.active_tasks.get_nodes() + if sched_cb.running_task.is_some() {1} else {0}
}

pub fn get_num_waiting_tasks() -> usize {
    SCHEDULER_CON_BLK.local().lock().waiting_tasks.get_nodes()
}

pub fn get_num_terminated_tasks() -> usize {
    SCHEDULER_CON_BLK.local().lock().terminated_tasks.get_nodes()
}

pub fn yield_cpu() {
    // Remove all remaining run time
    get_current_task()
    .expect("yield_cpu() called from idle task!").lock().quanta = 0;

    hal::yield_cpu();
}

pub fn init() {
    let init_task = Task::new(false, 0)
    .expect("Init task creation failed!!");

    init_task.lock().status = TaskStatus::RUNNING;
    init_task.lock().panic_base = get_panic_base();

    {
        let mut sched_cb = SCHEDULER_CON_BLK.local().lock();
        sched_cb.active_tasks.add_node(init_task).expect("Init task creation failed!");

        let task = NonNull::from(sched_cb.active_tasks.first().unwrap());

        unsafe {
            let guard = sched_cb.active_tasks.remove_node(task);
            sched_cb.running_task = Some(ListNode::into_inner(guard));
        }
    }

    // Now init idle task stack for all cpus
    for core in 0..get_total_cores() {
        let stack_base = get_worker_stack(core);
        let mut sched_cb = unsafe {
            SCHEDULER_CON_BLK.get(core).lock()
        };

        // We need to create separate stack for idle task on cpu 0, since the current stack is used by init task
        if core == 0 {
            let stack = Stack::into_inner(&mut Stack::new().expect("Could not create worker stack for cpu 0"));
            sched_cb.idle_task_stack = stack; 
        }
        else {
            sched_cb.idle_task_stack = NonNull::new(stack_base as *mut u8).unwrap();
        }
    } 

    enable_scheduler_timer();
}

pub fn add_cur_task_to_wait_queue(wait_semaphore: KSemInnerType) {
    let cur_task = get_current_task()
    .expect("add_cur_task_to_wait_queue() called from idle task!!");
    let mut task = cur_task.lock();
    // TERMINATED > WAITING, don't do anything
    if task.status == TaskStatus::TERMINATED {
        return;
    }

    task.wait_semaphores.add_node(wait_semaphore)
    .expect("System in bad state.. Could not add semaphore to task list");

    task.status = TaskStatus::WAITING;
}

fn remove_wait_semaphore(task: &mut Task, wait_semaphore: KSemInnerType) {
    let mut sem = None;
    let sem_val = (&*wait_semaphore) as *const _;

    for semaphore in task.wait_semaphores.iter() {
        let val = (&***semaphore) as *const _;

        if val == sem_val {
            sem = Some(NonNull::from(semaphore));
            break;
        } 
    }

    if sem.is_some() {
        unsafe {
            task.wait_semaphores.remove_node(sem.unwrap());
        }
    }
}


pub fn signal_waiting_task(task_id: usize, wait_semaphore: KSemInnerType) {
    let this_task = get_task_info(task_id);

    // It could be that this task has been killed
    if this_task.is_none() {
        return;
    }

    let this_task = this_task.unwrap();

    let mut sched_cb = unsafe {
        SCHEDULER_CON_BLK.get(this_task.lock().core).lock() 
    };

    // TERMINATED > WAITING, don't do anything
    if this_task.lock().status == TaskStatus::TERMINATED {
        return;
    }

    let mut waiting_task= None;
    for task in sched_cb.waiting_tasks.iter() {
        if task.lock().get_id() == task_id {
            waiting_task = Some(NonNull::from(task));
            break;
        }
    }

    // This happens when signal task is called even before the waiting task gets a chance to be put into the wait queue
    if waiting_task.is_none() {
        let mut task = unsafe {
            (*sched_cb.running_task.unwrap().as_ptr()).lock()
        };

        assert!(task.status == TaskStatus::WAITING);

        // Let task run again with high priority
        task.status = TaskStatus::RUNNING;
        task.quanta = INIT_QUANTA;

        notify_other_cpu(task.core);
        remove_wait_semaphore(&mut *task, wait_semaphore);
        return;
    }

    let signal_task = unsafe {
        ListNode::into_inner(sched_cb.waiting_tasks.remove_node(waiting_task.unwrap()))
    };

    let mut task = unsafe {
        (**signal_task.as_ptr()).lock()
    };

    task.status = TaskStatus::ACTIVE;

    remove_wait_semaphore(&mut *task, wait_semaphore);

    // Give this task the highest priority
    sched_cb.active_tasks.insert_node_at_head(signal_task);
    notify_other_cpu(task.core);
}

pub fn kill_task(task_id: usize) {
    let mut yield_flag = false;
    let mut drop_task  = false;
    let this_task = get_task_info(task_id);

    if this_task.is_none() {
        return;
    }

    let this_task: Arc<Spinlock<Task>, PoolAllocatorGlobal> = this_task.unwrap();
    let core = this_task.lock().core;

    {
        let mut sched_cb = unsafe {
            SCHEDULER_CON_BLK.get(core).lock()
        };

        let status = this_task.lock().status;
        if status == TaskStatus::TERMINATED {
            return;
        }

        this_task.lock().status = TaskStatus::TERMINATED;

        // Remove task from active list and add to terminated list
        match status {
            TaskStatus::ACTIVE => {
                let mut task_l: Option<NonNull<ListNode<Arc<Spinlock<Task>, PoolAllocatorGlobal>>>> = None;
                for active_task in sched_cb.active_tasks.iter() {
                    if active_task.lock().id == task_id {
                        task_l = Some(NonNull::from(active_task));
                        break;
                    }
                }

                assert!(task_l.is_some());
                let task_node = unsafe {
                    ListNode::into_inner(sched_cb.active_tasks.remove_node(task_l.unwrap()))
                };

                sched_cb.terminated_tasks.insert_node_at_tail(task_node);
            },

            TaskStatus::WAITING => {
                let mut task_l = None;
                for waiting_task in sched_cb.waiting_tasks.iter() {
                    if waiting_task.lock().id == task_id {
                        task_l = Some(NonNull::from(waiting_task));
                        break;
                    }
                }

                if task_l.is_some() {
                    let task_node = unsafe {
                        ListNode::into_inner(sched_cb.waiting_tasks.remove_node(task_l.unwrap()))
                    };

                    sched_cb.terminated_tasks.insert_node_at_tail(task_node);

                }
                else {
                    // Task might not have been scheduled out
                    // In this case, let scheduler take care of it

                    // We might be here due to interrupt in same cpu / kill task issued by different cpu
                }

                drop_task = true;
            },

            TaskStatus::RUNNING => {
                // The current task is killing itself (exit)
                yield_flag = true;
                
            },

            _ => {
                panic!("TaskStatus::TERMINATED state cannot be encountered!!");
            }
        }
    }

    notify_other_cpu(core);

    // Inform semaphore that this task is about to be killed, remove it from the blocked list
    if drop_task {
        while this_task.lock().wait_semaphores.get_nodes() != 0 {
            // We do it in this weird fashion since we don't want the task to be locked during call to drop_task
            let (sem_wrap, sem_inner) = {
                let task = this_task.lock();
                let sem = task.wait_semaphores.first();
                (Arc::clone(&**sem.unwrap()), NonNull::from(sem.unwrap()))
            };

            KSem::drop_task(sem_wrap, task_id);

            unsafe {
                this_task.lock().wait_semaphores.remove_node(sem_inner);
            }
        } 
    }

    // The current running task is killed, yield remaining context
    if yield_flag {
        // Yielding tricks rust compiler into thinking that
        // the stack frame is preserved, thereby not releasing the task
        // So, explicitly drop it
        drop(this_task);
        yield_cpu();
    }
}

// We do all this moving out of stuff and into other stuff drama in order to avoid holding any lock during signal operation
fn reap_tasks(mut term_list: DynList<KThread>) {
    while term_list.get_nodes() != 0 {
        let task = NonNull::from(term_list.first().unwrap());
        let task_inner = unsafe {
            &*task.as_ptr()
        };
        
        task_inner.signal();
        let id = task_inner.lock().get_id();

        info!("Removing task {}", id);
        unsafe {
            term_list.remove_node(task);
        }
        
        TASKS.lock().remove(&id);
    }   
}

// Main scheduler loop
pub fn schedule() {
    let term_tasks = {
        let mut sched_cb = SCHEDULER_CON_BLK.local().lock();

        if sched_cb.running_task.is_some() {
            let current_task = sched_cb.running_task.unwrap(); 
            let mut task_info = unsafe {
                current_task.as_ref().lock()
            };

            task_info.quanta = task_info.quanta.saturating_sub(1);
            
            // Switch to new task
            if task_info.status == TaskStatus::WAITING || task_info.status == TaskStatus::TERMINATED ||
            task_info.quanta == 0 {
                // First choose new task
                // We create NonNull here so that the node can later be removed
                let head_task = sched_cb.active_tasks.first().and_then(|item| {
                    Some(NonNull::from(item))
                });

                if head_task.is_some() {
                    let mut head_task_info = unsafe {
                        head_task.unwrap().as_ref().lock()
                    };
                    
                    assert!(head_task_info.status == TaskStatus::ACTIVE); 
                    head_task_info.status = TaskStatus::RUNNING;
                    head_task_info.quanta = INIT_QUANTA;
                    let new_context = head_task_info.context;
                    
                    let prev_context = fetch_context();
                    task_info.context = prev_context;
                    if task_info.status == TaskStatus::RUNNING {
                        task_info.status = TaskStatus::ACTIVE; 
                    }

                    // This ensures that list doesn't delete the node. It simply removes it from the list 
                    let head_task = unsafe {
                        ListNode::into_inner(sched_cb.active_tasks.remove_node(head_task.unwrap()))
                    };

                    if task_info.status == TaskStatus::WAITING {
                        sched_cb.waiting_tasks.insert_node_at_tail(current_task);
                    }
                    else if task_info.status == TaskStatus::TERMINATED {
                        sched_cb.terminated_tasks.insert_node_at_tail(current_task);
                    }
                    else {
                        sched_cb.active_tasks.insert_node_at_tail(current_task);
                    }

                    sched_cb.running_task = Some(head_task);

                    set_panic_base(head_task_info.panic_base);
                    switch_context(new_context);
                }
                else {
                    if task_info.status != TaskStatus::RUNNING {
                        let prev_context = fetch_context();
                        task_info.context = prev_context;

                        if task_info.status == TaskStatus::WAITING {
                            sched_cb.waiting_tasks.insert_node_at_tail(current_task);
                        }
                        else if task_info.status == TaskStatus::TERMINATED {
                            sched_cb.terminated_tasks.insert_node_at_tail(current_task);
                        }
                        prep_idle_task(&mut sched_cb);
                    }
                    else {
                        // No other task to run. Continue with this task
                        task_info.quanta = INIT_QUANTA;
                    }
                }
            }
        }
        else {
            // This means we're in idle task. Check and run any active tasks
            let head_task = sched_cb.active_tasks.first().and_then(|item| {
                Some(NonNull::from(item))
            });

            debug!("Moving out of idle task on core {}", hal::get_core());

            if head_task.is_some() {
                let mut head_task_info = unsafe {
                    head_task.unwrap().as_ref().lock()
                };

                assert!(head_task_info.status == TaskStatus::ACTIVE); 
                head_task_info.status = TaskStatus::RUNNING;
                head_task_info.quanta = INIT_QUANTA;
                let new_context = head_task_info.context;
                
                let head_task = unsafe {
                    ListNode::into_inner(sched_cb.active_tasks.remove_node(head_task.unwrap()))
                };
                sched_cb.running_task = Some(head_task);

                set_panic_base(head_task_info.panic_base);
                switch_context(new_context);
                debug!("Moved out of idle task on core {}", hal::get_core());
            }
            else {
                disable_scheduler_timer(); 
            }
        }
        
        unsafe {
            sched_cb.terminated_tasks.move_list()
        }
    };

    reap_tasks(term_tasks);
}

fn prep_idle_task(sched_cb: &mut TaskQueue) {
    sched_cb.running_task = None;
    let context = create_kernel_context(idle_task, sched_cb.idle_task_stack.as_ptr() as *mut u8);
    set_panic_base(sched_cb.idle_task_stack.as_ptr() as usize);
    switch_context(context);
    
    disable_scheduler_timer(); 
}

fn idle_task() -> ! {
    info!("Moving to idle task on core {}", hal::get_core());
    hal::sleep();
}

fn notify_other_cpu(target_core: usize) {
    if hal::get_core() == target_core {
        enable_scheduler_timer();
        return;
    }

    info!("Notifying cpu {} on new task", target_core);
    let _ = hal::notify_core(IPIRequestType::SchedChange, target_core);
}

pub fn create_task(handler: fn() -> !) -> Result<KThread, KError> {
    let core = TASK_CPU.fetch_add(1, Ordering::Relaxed) as usize % get_total_cores();   
    let task = Task::new(true, core)?;
    
    {
        let mut task = task.lock();
        let stack_base = task.stack.as_ref().unwrap().get_stack_base();

        // Setup the initial context
        let context = create_kernel_context(handler, stack_base as *mut u8);
        task.context = context;  
        task.panic_base = stack_base;
    }

    // We will use simple round robin to determine the cpu which gets this task
    unsafe {
        // Add to ready queue
        let mut sched_cb = SCHEDULER_CON_BLK.get(core).lock();
        sched_cb.active_tasks.add_node(Arc::clone(&task))?;
    };

    notify_other_cpu(core);

    Ok(task)
}


impl Spinlock<Task> {
    fn signal(&self) {
        let sem = {
            let task = self.lock();
            task.term_notify.clone() 
        };

        sem.signal();
    }

    pub fn wait(&self) -> Result<(), KError> {
        let sem = {
            let task = self.lock();
            task.term_notify.clone() 
        };

        sem.wait()
    }
}