use core::ptr::NonNull;
use super::Spinlock;
use crate::{ds::*, sched};
use kernel_intf::KError;
use crate::sched::KThread;

struct KSemInner {
    max_count: usize,
    counter: isize,
    blocked_list: DynList<KThread>
}

pub struct KSem {
    inner: Spinlock<KSemInner>
}

unsafe impl Sync for KSem {}
unsafe impl Send for KSem {}

impl KSem {
    pub const fn new(init_count: isize, max_count: usize) -> Self {
        Self {
            inner: Spinlock::new(KSemInner {
                max_count,
                counter: init_count,
                blocked_list: List::new()
            }) 
        }
    }

    pub fn wait(&self) -> Result<(), KError> {
        {
            let mut inner = self.inner.lock();
            let count = inner.counter;
            inner.counter -= 1;
            
            let cur_task = sched::get_current_task();

            if count <= 0 {
                // Block task
                inner.blocked_list.add_node(cur_task)?;
                sched::add_cur_task_to_wait_queue();
            }
        }

        // We call it here, in order to unlock the spinlock
        sched::yield_cpu();

        Ok(())
    }

    pub fn signal(&self) {
        {
            let mut inner = self.inner.lock();
            inner.counter = (inner.max_count as isize).min(inner.counter + 1);

            if inner.counter >= 0 {
                let wait_count = inner.blocked_list.get_nodes();
                
                // Remove head task from blocked list
                if wait_count > 0 {
                    let wait_task_ptr = NonNull::from(inner.blocked_list.first().unwrap());
                    let node = unsafe {
                        inner.blocked_list.remove_node(wait_task_ptr)
                    };
                    
                    let id = node.lock().get_id();
                    sched::signal_waiting_task(id);
                } 
            }
        }
    }
}