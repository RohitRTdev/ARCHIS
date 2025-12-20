use alloc::sync::Arc;
use crate::cpu::{MAX_CPUS, PerCpu, Stack, get_panic_base, set_panic_base};
use crate::hal::{self, create_kernel_context, fetch_context, register_timer_fn, switch_context};
use crate::mem::PoolAllocatorGlobal;
use crate::{ds::*, sched};
use crate::sync::Spinlock;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::ptr::NonNull;
use kernel_intf::{KError, debug, info};

// This is in milliseconds
pub const QUANTUM: usize = 5;
const INIT_QUANTA: usize = 10;

pub type KThread = Arc<Spinlock<Task>, PoolAllocatorGlobal>;

static TASK_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, PartialEq)]
pub enum TaskStatus {
    RUNNING,
    ACTIVE,
    WAITING
}

#[derive(Debug)]
pub struct Task {
    id: usize,
    stack: *const Stack,
    status: TaskStatus,
    context: *const u8,
    quanta: usize,
    panic_base: usize
}

impl Task {
    fn new() -> KThread {
        let id = TASK_ID.fetch_add(1, Ordering::Relaxed);  
        let stack  = Stack::new();
        debug!("Creating task with ID:{} and stack_addr={:#X}", id, unsafe {(*stack).get_stack_base()});

        Arc::new_in(Spinlock::new(Task {
            id, 
            stack,
            status: TaskStatus::ACTIVE,
            context: core::ptr::null(),
            quanta: INIT_QUANTA,
            panic_base: 0
        }), PoolAllocatorGlobal)
    }

    pub fn get_id(&self) -> usize {
        self.id
    }
}

pub struct TaskQueue {
    active_tasks: DynList<KThread>,
    waiting_tasks: DynList<KThread>,
    running_task: Option<NonNull<ListNode<KThread>>>
}

unsafe impl Send for TaskQueue{}

impl TaskQueue {
    const fn new() -> Self {
        TaskQueue {
            active_tasks: List::new(),
            waiting_tasks: List::new(),
            running_task: None
        }
    }
}

static SCHEDULER_CON_BLK: PerCpu<Spinlock<TaskQueue>> = PerCpu::new_with(
    [const {Spinlock::new(TaskQueue::new())}; MAX_CPUS]
);

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

pub fn add_cur_task_to_wait_queue() {
    let sched_cb = SCHEDULER_CON_BLK.local().lock();

    // We will add support for idle task later
    assert!(sched_cb.running_task.is_some());    
    unsafe {
        (**sched_cb.running_task.unwrap().as_ptr()).lock().status = TaskStatus::WAITING;
    }
}

pub fn signal_waiting_task(task_id: usize) {
    let mut sched_cb = SCHEDULER_CON_BLK.local().lock();

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

        // Let task run again with high priority
        task.status = TaskStatus::RUNNING;
        task.quanta = INIT_QUANTA;
        return;
    }

    let signal_task = unsafe {
        ListNode::into_inner(sched_cb.waiting_tasks.remove_node(waiting_task.unwrap()))
    };

    let mut task = unsafe {
        (**signal_task.as_ptr()).lock()
    };

    task.status = TaskStatus::ACTIVE;

    // Give this task the highest priority
    sched_cb.active_tasks.insert_node_at_head(signal_task);
}



// Select the ready/active task at head of queue
// Run the task if it has non-zero quanta left
fn schedule() {
    let mut sched_cb = SCHEDULER_CON_BLK.local().lock();

    // We ensure that we don't encounter the scenario where there is no running task but tasks exist in the ready queue
    if sched_cb.running_task.is_some() {
        let current_task = sched_cb.running_task.unwrap(); 
        let mut task_info = unsafe {
            current_task.as_ref().lock()
        };

        task_info.quanta = task_info.quanta.saturating_sub(1);
        
        // Switch to new task
        if task_info.status == TaskStatus::WAITING || task_info.quanta == 0 {
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
                if task_info.status != TaskStatus::WAITING {
                    task_info.status = TaskStatus::ACTIVE; 
                }

                // This ensures that list doesn't delete the node. It simply removes it from the list 
                let head_task = unsafe {
                    ListNode::into_inner(sched_cb.active_tasks.remove_node(head_task.unwrap()))
                };

                if task_info.status == TaskStatus::WAITING {
                    sched_cb.waiting_tasks.insert_node_at_tail(current_task);
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
                assert!(task_info.status != TaskStatus::WAITING, "Idle task not supported right now");
                task_info.quanta = INIT_QUANTA;
            }
        }
    }
    else {
        // This means we're in idle task. Don't do anything
    }
}

fn idle_task() -> ! {
    hal::sleep();
}

pub fn create_task(handler: fn() -> !) -> Result <(), KError> {
    let task = Task::new();
    let stack_base = unsafe {
        (*task.lock().stack).get_stack_base()
    };

    // Setup the initial context
    let context = create_kernel_context(handler, stack_base as *mut u8);
    task.lock().context = context;  
    task.lock().panic_base = stack_base;

    // Add to ready queue
    SCHEDULER_CON_BLK.local().lock().active_tasks.add_node(task)
}