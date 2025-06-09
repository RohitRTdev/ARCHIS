use crate::mem::Allocator;
use core::alloc::Layout;
use core::mem;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::marker::PhantomData;

pub struct ListIter<'a, T> {
    current: Option<&'a ListNode<T>>,
    head: Option<&'a ListNode<T>>
}

pub struct ListIterMut<'a, T> {
    current: Option<*mut ListNode<T>>,
    head: Option<*mut ListNode<T>>,
    _marker: PhantomData<&'a mut ListNode<T>>
}

pub struct ListNode<T> {
    data: T,
    prev: NonNull<ListNode<T>>,
    next: NonNull<ListNode<T>>
}

pub struct ListNodeGuard<T, A: Allocator<ListNode<T>>> {
    guard: NonNull<ListNode<T>>,
    _marker: PhantomData<A>
}



impl<T, A: Allocator<ListNode<T>>> ListNodeGuard<T, A> {
    pub fn into_inner(guard_node: Self) -> NonNull<ListNode<T>> {
        let guard_node = mem::ManuallyDrop::new(guard_node);
        guard_node.guard
    }
}

pub struct List<T, A: Allocator<ListNode<T>>> {
    head: Option<*mut ListNode<T>>,
    tail: Option<*mut ListNode<T>>,
    num_nodes: usize,
    _marker: PhantomData<A>
}

impl<T, A: Allocator<ListNode<T>>> Deref for ListNodeGuard<T, A> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe {
            self.guard.as_ref()
        }
    }
}

impl<T, A: Allocator<ListNode<T>>> DerefMut for ListNodeGuard<T, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            self.guard.as_mut()
        }
    }
}

impl<T> Deref for ListNode<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for ListNode<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T, A: Allocator<ListNode<T>>> Drop for ListNodeGuard<T, A> {
    fn drop(&mut self) {
        unsafe {
            A::dealloc(self.guard, Layout::for_value(self.guard.as_ref()));
        }
    }
}

impl<T, A: Allocator<ListNode<T>>> List<T, A> {
    pub fn new() -> Self {
        List {
            head: None,
            tail: None,
            num_nodes: 0,
            _marker: PhantomData
        }
    }

    pub fn first(&self) -> Option<&ListNode<T>> {
        if self.head.is_none() {
            None
        }
        else {
            unsafe {
                Some(&*self.head.unwrap())
            }
        }
    }

    pub fn add_node(&mut self, data: T) {
        let layout = Layout::from_size_align(size_of::<ListNode<T>>(), align_of::<ListNode<T>>()).unwrap();
        let addr = A::alloc(layout).as_ptr();
        let addr_non = NonNull::new(addr).unwrap();
        unsafe {
            (*addr).next = addr_non;
            (*addr).prev = addr_non;
            (*addr).data = data;
        }

        self.insert_node_at_tail(addr_non);
    }


    pub fn get_nodes(&self) -> usize {
        self.num_nodes
    }

    fn insert_node(&mut self, this: NonNull<ListNode<T>>, insert_at_tail: bool) {
        let this_node = unsafe {
            &mut *this.as_ptr()
        };

        let this_opt = Some(this.as_ptr());

        if self.num_nodes == 0 {
            self.head = this_opt;
            self.tail = this_opt;
            this_node.next = this;
            this_node.prev = this;
        }
        else {
            let tail_node = unsafe {
                &mut *self.tail.unwrap()
            };
            let head_node = unsafe {
                &mut *self.head.unwrap()
            };
            
            tail_node.next = this;
            this_node.prev = NonNull::new(self.tail.unwrap()).unwrap();
            this_node.next = NonNull::new(self.head.unwrap()).unwrap();
            head_node.prev = this;
            if insert_at_tail {
                self.tail = this_opt;
            }
            else {
                self.head = this_opt;
            }
        }

        self.num_nodes += 1;
    }

    pub fn insert_node_at_tail(&mut self, this: NonNull<ListNode<T>>) {
        self.insert_node( this, true);
    }
    
    pub fn insert_node_at_head(&mut self, this: NonNull<ListNode<T>>) {
        self.insert_node(this, false);
    }

    pub unsafe fn remove_node(&mut self, this: NonNull<ListNode<T>>) -> ListNodeGuard<T, A> {
        let this_node = unsafe {
            &mut *this.as_ptr()
        };

        if self.num_nodes == 1 {
            self.head = None;
            self.tail = None;
        }
        else {
            let prev_node = unsafe {
                &mut *this_node.prev.as_ptr()
            };
            let next_node = unsafe {
                &mut *this_node.next.as_ptr()
            };

            prev_node.next = this_node.next;
            next_node.prev = this_node.prev;

            if self.head.unwrap() == this.as_ptr() {
                self.head = Some(this_node.next.as_ptr());
            }
            else if self.tail.unwrap() == this.as_ptr() {
                self.tail = Some(this_node.prev.as_ptr());
            }
        }
        
        self.num_nodes -= 1; 

        ListNodeGuard {guard: this, _marker: PhantomData}
    }
    
    pub fn iter(&self) -> ListIter<'_, T> {
        if let Some(head) = self.head {
            ListIter {
                current: unsafe {Some(&*head)},
                head: unsafe {Some(&*head)}
            }
        }
        else {
            ListIter {
                current: None,
                head: None
            }
        }
    }

    pub fn iter_mut(&mut self) -> ListIterMut<T> {
        if let Some(head) = self.head {
            ListIterMut {
                current: Some(head),
                head: Some(head),
                _marker: PhantomData
            }
        }
        else {
            ListIterMut {
                current: None,
                head: None,
                _marker: PhantomData
            }
        }
    }

}


impl<'a, T> Iterator for ListIter<'a, T> {
    type Item = &'a ListNode<T>;
    fn next(&mut self) -> Option<Self::Item> {        
        if self.current.is_some() {
            let node = self.current;
            let next = (*self.current.unwrap()).next;

            // Since it's circular list, we have reached end if we are at head node. So stop iterating.
            if next.as_ptr() == self.head.unwrap() as *const ListNode<T> as *mut ListNode<T> {
                self.current = None;
            }
            else {
                self.current = Some(unsafe {&*next.as_ptr()});
            }

            node 
        }
        else {
            None
        }
    }
}

impl<'a, T> Iterator for ListIterMut<'a, T> {
    type Item = &'a mut ListNode<T>;
    fn next(&mut self) -> Option<Self::Item> {        
        if self.current.is_some() {
            let node = self.current;
            let next = unsafe {
                (*self.current.unwrap()).next
            };

            // Since it's circular list, we have reached end if we are at head node. So stop iterating.
            if next.as_ptr() == self.head.unwrap() as *const ListNode<T> as *mut ListNode<T> {
                self.current = None;
            }
            else {
                self.current = Some(next.as_ptr());
            }

            unsafe {
                Some(&mut *node.unwrap())
            }
        }
        else {
            None
        }
    }
}

