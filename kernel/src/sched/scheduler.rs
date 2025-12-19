use alloc::sync::Arc;
use crate::cpu::{MAX_CPUS, PerCpu, Stack, get_panic_base, set_panic_base};
use crate::hal::{self, create_kernel_context, fetch_context, register_timer_fn, switch_context};
use crate::{ds::*, sched};
use crate::sync::Spinlock;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::ptr::NonNull;
use kernel_intf::{KError, debug, info};

// This is in milliseconds
pub const QUANTUM: usize = 10;
const INIT_QUANTA: usize = 10;
    
static TASK_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub enum TaskStatus {
    RUNNING,
    ACTIVE
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
    fn new() -> Arc<Spinlock<Task>> {
        let id = TASK_ID.fetch_add(1, Ordering::Relaxed);  
        let stack  = Stack::new();
        debug!("Creating task with ID:{} and stack_addr={:#X}", id, unsafe {(*stack).get_stack_base()});

        Arc::new(Spinlock::new(Task {
            id, 
            stack,
            status: TaskStatus::ACTIVE,
            context: core::ptr::null(),
            quanta: INIT_QUANTA,
            panic_base: 0
        }))
    }
}

pub struct TaskQueue {
    active_tasks: DynList<Arc<Spinlock<Task>>>,
    running_task: Option<NonNull<ListNode<Arc<Spinlock<Task>>>>>
}

unsafe impl Send for TaskQueue{}

impl TaskQueue {
    const fn new() -> Self {
        TaskQueue {
            active_tasks: List::new(),
            running_task: None
        }
    }
}

static SCHEDULER_CON_BLK: PerCpu<Spinlock<TaskQueue>> = PerCpu::new_with(
    [const {Spinlock::new(TaskQueue::new())}; MAX_CPUS]
);

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
        //info!("Called schedule on task:{} with quanta: {}", task_info.id, task_info.quanta);

        // Switch to new task
        if task_info.quanta == 0 {
            //info!("About to schedule switch out of task:{}", task_info.id);
            // First choose new task
            // We create NonNull here so that the node can later be removed
            let head_task = sched_cb.active_tasks.first().and_then(|item| {
                Some(NonNull::from(item))
            });
            
            //info!("head_task:{:?}", head_task);

            if head_task.is_some() {
                let mut head_task_info = unsafe {
                    head_task.unwrap().as_ref().lock()
                };
                
                head_task_info.status = TaskStatus::RUNNING;
                head_task_info.quanta = INIT_QUANTA;
                let new_context = head_task_info.context;
                
                let prev_context = fetch_context();
                task_info.context = prev_context;
                task_info.status = TaskStatus::ACTIVE;

                // This ensures that list doesn't delete the node. It simply removes it from the list 
                let head_task = unsafe {
                    ListNode::into_inner(sched_cb.active_tasks.remove_node(head_task.unwrap()))
                };

                //info!("head_task_info:{:?}", &*head_task_info);
                sched_cb.active_tasks.insert_node_at_tail(current_task);
                sched_cb.running_task = Some(head_task);

                set_panic_base(head_task_info.panic_base);
                switch_context(new_context);
            }
            else {
                // No other task to run. Continue with this task
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