use crate::mem::Allocator;
use core::ptr;
use core::alloc::Layout;
use core::ffi::c_void;
use core::marker::PhantomData;
struct ListNode<T> {
    data: T,
    prev: *mut ListNode<T>,
    next: *mut ListNode<T>
}

struct List<T, A: Allocator> {
    head: *mut ListNode<T>,
    tail: *mut ListNode<T>,
    num_nodes: usize,
    _marker: PhantomData<A>
}


impl<T, A: Allocator> List<T, A> {
    pub fn new() -> Self {
        List {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
            num_nodes: 0,
            _marker: PhantomData
        }
    }

    pub fn add_node(&mut self, data: T) {
        let node =  ListNode {
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
            data
        };

        let addr = A::alloc(Layout::for_value(&node)) as *mut ListNode<T>;

        self.insert_node_at_tail(addr);
    }

    pub unsafe fn delete_node(this: *mut ListNode<T>) {
        A::dealloc(this as *mut c_void);
    }

    fn insert_node(&mut self, this: *mut ListNode<T>, insert_at_tail: bool) {
        let this_node = unsafe {
            &mut *this
        };

        if self.num_nodes == 0 {
            self.head = this;
            self.tail = this;
            this_node.next = this;
            this_node.prev = this;
        }
        else {
            let tail_node = unsafe {
                &mut *self.tail
            };
            let head_node = unsafe {
                &mut *self.head
            };
            tail_node.next = this;
            this_node.prev = self.tail;
            this_node.next = self.head;
            head_node.prev = this_node;
            if insert_at_tail {
                self.tail = this;
            }
            else {
                self.head = this;
            }
        }

        self.num_nodes += 1;
    }

    pub fn insert_node_at_tail(&mut self, this: *mut ListNode<T>) {
        self.insert_node( this, true);
    }
    
    pub fn insert_node_at_head(&mut self, this: *mut ListNode<T>) {
        self.insert_node(this, false);
    }

    pub unsafe fn remove_node(&mut self, this: *mut ListNode<T>) {
        let this_node = unsafe {
            &mut *this
        };

        if self.num_nodes == 1 {
            self.head = ptr::null_mut();
            self.tail = ptr::null_mut();
        }
        else {
            let prev_node = unsafe {
                &mut *this_node.prev
            };
            let next_node = unsafe {
                &mut *this_node.next
            };

            prev_node.next = this_node.next;
            next_node.prev = this_node.prev;

            if self.head == this {
                self.head = this_node.next;
            }
            else if self.tail == this {
                self.tail = this_node.prev;
            }
        }
        
        self.num_nodes -= 1; 
    }
}