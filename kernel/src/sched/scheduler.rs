use alloc::sync::Arc;
use crate::cpu::{MAX_CPUS, PerCpu, Stack};
use crate::hal::{self, register_timer_fn};
use crate::ds::*;
use crate::sync::Spinlock;
use kernel_intf::info;

// This is in milliseconds
pub const QUANTUM: usize = 10;


pub enum TaskStatus {
    RUNNING,
    ACTIVE
}

pub struct Task {
    id: usize,
    stack: Stack,
    status: TaskStatus,
    context: *const u8
}

// This is fine, since we're never really going to dereference the context pointer
// It is declared as a pointer only for ergonomic reasons
unsafe impl Sync for Task{}
unsafe impl Send for Task{}

pub struct TaskQueue {
    active_tasks: DynList<Arc<Task>>,
    running_task: Option<Arc<Task>>
}

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
    register_timer_fn(schedule);
}


// Select the ready/active task at head of queue
// Run the task if it has non-zero quanta left
fn schedule() {
}

fn idle_task() -> ! {
    hal::sleep();
}

pub fn create_task() {

}