use super::Spinlock;
use crate::{ds::*, sched};
use alloc::sync::Arc;
use kernel_intf::{KError, debug};
use crate::sched::Task;

struct KSemInner {
    counter: isize,
    blocked_list: DynList<Arc<Spinlock<Task>>>,
    running_list: DynList<Arc<Spinlock<Task>>>
}

pub struct KSem {
    inner: Spinlock<KSemInner>
}

unsafe impl Sync for KSem {}
unsafe impl Send for KSem {}

impl KSem {
    pub const fn new(init_count: isize) -> Self {
        Self {
            inner: Spinlock::new(KSemInner {
                counter: init_count,
                blocked_list: List::new(),
                running_list: List::new()
            }) 
        }
    }


    pub fn wait(&self) -> Result<(), KError> {
        {
            let mut inner = self.inner.lock();
            let count = inner.counter;
            inner.counter -= 1;
            
            let cur_task = sched::get_current_task();

            let task_id = cur_task.lock().get_id();

            if count <= 0 {
                // Block task
                inner.blocked_list.add_node(cur_task)?;

                debug!("Placing task id:{} into wait queue", task_id);
                sched::add_cur_task_to_wait_queue();
            }
            else {
                debug!("Placing task id:{} into running queue", task_id);
                return inner.running_list.add_node(cur_task);
            }
        }

        // We call it here, in order to unlock the spinlock
        sched::yield_cpu();

        Ok(())
    }

    pub fn signal(&self) {

    }
}