use super::*;
use crate::mem::Allocator;
use kernel_intf::KError;
use core::ptr::NonNull;

pub struct Queue<T, A: Allocator<ListNode<T>>> {       
    data: List<T, A>
}

impl<T, A: Allocator<ListNode<T>>> Queue<T, A> {
    pub fn new() -> Self {
        Queue {
            data: List::new()
        }
    }

    pub fn push(&mut self, item: T) -> Result<(), KError> {
        self.data.add_node(item)
    }

    pub fn push_node(&mut self, item: NonNull<ListNode<T>>) {
        self.data.insert_node_at_tail(item);
    }

    pub fn pop_node(&mut self) -> Option<ListNodeGuard<T, A>> {
        let head = self.data.first();
        if head.is_some() {
            unsafe {
                Some(self.data.remove_node(NonNull::from(head.unwrap())))
            }
        }
        else {
            None
        }
    }
}