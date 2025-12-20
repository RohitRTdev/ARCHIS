use alloc::sync::Arc;
use alloc::task;
use crate::cpu::{MAX_CPUS, PerCpu, Stack, get_panic_base, set_panic_base};
use crate::hal::{self, create_kernel_context, fetch_context, register_timer_fn, switch_context};
use crate::mem::PoolAllocatorGlobal;
use crate::{ds::*, sched};
use crate::sync::{KSem, KSemInnerType, Spinlock};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::ptr::NonNull;
use alloc::collections::BTreeMap;
use kernel_intf::{KError, debug, info};

// This is in milliseconds
pub const QUANTUM: usize = 5;
const INIT_QUANTA: usize = 2;

pub type KThread = Arc<Spinlock<Task>, PoolAllocatorGlobal>;

static TASK_ID: AtomicUsize = AtomicUsize::new(0);
static TASKS: Spinlock<BTreeMap<usize, KThread>> = Spinlock::new(BTreeMap::new());

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TaskStatus {
    RUNNING,
    ACTIVE,
    WAITING,
    TERMINATED
}

pub struct Task {
    id: usize,
    stack: *const Stack,
    status: TaskStatus,
    context: *const u8,
    quanta: usize,
    panic_base: usize,
    wait_semaphores: DynList<KSemInnerType>
}

impl Task {
    fn new() -> KThread {
        let id = TASK_ID.fetch_add(1, Ordering::Relaxed);  
        let stack  = Stack::new();
        debug!("Creating task with ID:{} and stack_addr={:#X}", id, unsafe {(*stack).get_stack_base()});

        let task = Arc::new_in(Spinlock::new(Task {
            id, 
            stack,
            status: TaskStatus::ACTIVE,
            context: core::ptr::null(),
            quanta: INIT_QUANTA,
            panic_base: 0,
            wait_semaphores: List::new()
        }), PoolAllocatorGlobal);

        TASKS.lock().insert(id, Arc::clone(&task));

        task
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
        
        unsafe {
            (*(self.stack as *mut Stack)).destroy();
        }
    }
}

unsafe impl Send for Task {}

pub struct TaskQueue {
    active_tasks: DynList<KThread>,
    waiting_tasks: DynList<KThread>,
    terminated_tasks: DynList<KThread>,
    running_task: Option<NonNull<ListNode<KThread>>>
}

unsafe impl Send for TaskQueue{}

impl TaskQueue {
    const fn new() -> Self {
        TaskQueue {
            active_tasks: List::new(),
            waiting_tasks: List::new(),
            terminated_tasks: List::new(),
            running_task: None
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

// Shouldn't call this function from idle task (for now)
pub fn get_current_task() -> KThread {
    let cb = SCHEDULER_CON_BLK.local().lock().running_task;
    assert!(cb.is_some());
    
    Arc::clone(unsafe {
        &**cb.unwrap().as_ptr()
    })
}

pub fn get_num_active_tasks() -> usize {
    // +1 since we need to account for the running task
    SCHEDULER_CON_BLK.local().lock().active_tasks.get_nodes() + 1
}

pub fn get_num_waiting_tasks() -> usize {
    SCHEDULER_CON_BLK.local().lock().waiting_tasks.get_nodes()
}

pub fn get_num_terminated_tasks() -> usize {
    SCHEDULER_CON_BLK.local().lock().terminated_tasks.get_nodes()
}

pub fn yield_cpu() {
    // Remove all remaining run time
    get_current_task().lock().quanta = 0;

    hal::yield_cpu();
}

pub fn init() {
    let init_task = Task::new();
    init_task.lock().status = TaskStatus::RUNNING;
    init_task.lock().panic_base = get_panic_base();

    let mut sched_cb = SCHEDULER_CON_BLK.local().lock();
    sched_cb.active_tasks.add_node(init_task).expect("Init task creation failed!");

    let task = NonNull::from(sched_cb.active_tasks.first().unwrap());

    unsafe {
        let guard = sched_cb.active_tasks.remove_node(task);
        sched_cb.running_task = Some(ListNode::into_inner(guard));
    }

    register_timer_fn(schedule);
}

pub fn add_cur_task_to_wait_queue(wait_semaphore: KSemInnerType) {
    let cur_task = get_current_task();
    let mut task = cur_task.lock();
    // TERMINATED > WAITING, don't do anything
    if task.status == TaskStatus::TERMINATED {
        return;
    }

    info!("Setting task {} status to waiting", task.get_id());
    task.wait_semaphores.add_node(wait_semaphore)
    .expect("System in bad state.. Could not add semaphore to task list");

    // We will add support for idle task later
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
        info!("Removing wait semaphore for task: {}", task.get_id());
        
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

    let mut sched_cb = SCHEDULER_CON_BLK.local().lock();

    // TERMINATED > WAITING, don't do anything
    if this_task.lock().status == TaskStatus::TERMINATED {
        return;
    }

    let mut waiting_task = None;
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
}

pub fn kill_task(task_id: usize) {
    let mut yield_flag = false;
    let mut drop_task  = false;
    let this_task = get_task_info(task_id);

    if this_task.is_none() {
        return;
    }

    let this_task = this_task.unwrap();

    {
        let mut sched_cb = SCHEDULER_CON_BLK.local().lock();

        let status = this_task.lock().status;
        if status == TaskStatus::TERMINATED {
            return;
        }

        this_task.lock().status = TaskStatus::TERMINATED;

        // Remove task from active list and add to terminated list
        match status {
            TaskStatus::ACTIVE => {
                info!("Killing active task");
                let mut task_l = None;
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
                    info!("Killing waiting task from wait list");
                    let task_node = unsafe {
                        ListNode::into_inner(sched_cb.waiting_tasks.remove_node(task_l.unwrap()))
                    };

                    sched_cb.terminated_tasks.insert_node_at_tail(task_node);

                }
                else {
                    info!("Killing waiting task still not scheduled out");
                    // Task might not have been scheduled out
                    // In this case, let scheduler take care of it

                    // We might be here due to interrupt in same cpu / kill task issued by different cpu
                }

                drop_task = true;
            },

            TaskStatus::RUNNING => {
                info!("Killing running task");
                
                // The current task is killing itself (exit)
                yield_flag = true;
            },

            _ => {
                panic!("TaskStatus::TERMINATED state cannot be encountered!!");
            }
        }
    }

    // Inform semaphore that this task is about to be killed, remove it from the blocked list
    if drop_task {
        let mut sem_id = 0;
        while this_task.lock().wait_semaphores.get_nodes() != 0 {
            // We do it in this weird fashion since we don't want the task to be locked during call to drop_task
            let (sem_wrap, sem_inner) = {
                let task = this_task.lock();
                let sem = task.wait_semaphores.first();
                (Arc::clone(&**sem.unwrap()), NonNull::from(sem.unwrap()))
            };

            info!("Removing wait semaphore {}", sem_id);
            sem_id += 1;
            KSem::drop_task(sem_wrap, task_id);

            unsafe {
                this_task.lock().wait_semaphores.remove_node(sem_inner);
            }
        } 
    }

    // The current running task is killed, yield remaining context
    if yield_flag {
        yield_cpu();
    }

}

fn reap_tasks(sched_cb: &mut TaskQueue) {
    while sched_cb.terminated_tasks.get_nodes() != 0 {
        let task = NonNull::from(sched_cb.terminated_tasks.first().unwrap());
        let id = unsafe {
            (*task.as_ptr()).lock().get_id()
        };

        unsafe {
            sched_cb.terminated_tasks.remove_node(task);
        }
        
        TASKS.lock().remove(&id);

        info!("Reaping terminated tasks");
        info!("Task map length = {}", TASKS.lock().len());
    }   
}

// Select the ready/active task at head of queue
// Run the task if it has non-zero quanta left
fn schedule() {
    let mut sched_cb = SCHEDULER_CON_BLK.local().lock();

    reap_tasks(&mut *sched_cb);

    // We ensure that we don't encounter the scenario where there is no running task but tasks exist in the ready queue
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
                // No other task to run. Continue with this task
                assert!(task_info.status == TaskStatus::RUNNING, "Idle task not supported right now");
                task_info.quanta = INIT_QUANTA;
            }
        }
    }
    else {
        // This means we're in idle task. Don't do anything (for now)
    }
}

fn idle_task() -> ! {
    hal::sleep();
}

pub fn create_task(handler: fn() -> !) -> Result<KThread, KError> {
    let task = Task::new();
    let stack_base = unsafe {
        (*task.lock().stack).get_stack_base()
    };

    // Setup the initial context
    let context = create_kernel_context(handler, stack_base as *mut u8);
    task.lock().context = context;  
    task.lock().panic_base = stack_base;

    // Add to ready queue
    SCHEDULER_CON_BLK.local().lock().active_tasks.add_node(Arc::clone(&task))?;

    Ok(task)
}