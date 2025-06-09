use core::ptr::NonNull;
use std::{sync::{Arc, Mutex, OnceLock}};

use crate::{ds, mem};

tests::init_test_logger!(aris);

#[derive(Debug)]
struct Sample {
    _a: i32,
    _b: u32
}

static TEST_LOCK: OnceLock<Arc<Mutex<bool>>> = OnceLock::new();

fn get_test_lock() -> &'static Arc<Mutex<bool>> {
    TEST_LOCK.get_or_init(|| Arc::new(Mutex::new(false)))
}


#[test]
fn fixed_allocator_test() {
    // Certain tests such as this needs to be run in isolation
    let _guard = get_test_lock().lock().unwrap();
    type Allocator = mem::FixedAllocator<Sample, {mem::Regions::Region0 as usize}>;
    
    use core::alloc::Layout;
    let mut layout = Layout::array::<Sample>(3).unwrap();   
    
    mem::clear_heap();
    let (heap, r0_bm) = mem::get_heap();

    // This should allocate first 3 slots in heap from region 0
    let ptr1 = <Allocator as mem::Allocator<Sample>>::alloc(layout);
    assert_eq!(ptr1.as_ptr() as *const u8, heap);
    assert_eq!(unsafe {*r0_bm}, 0x07);
    
    let ptr2 = <Allocator as mem::Allocator<Sample>>::alloc(layout);
    assert_eq!(ptr2.as_ptr() as *const u8, unsafe {heap.add(size_of::<Sample>() * 3)});
    assert_eq!(unsafe {*r0_bm}, 0x3f);
    unsafe {
        <Allocator as mem::Allocator<Sample>>::dealloc(ptr1, layout);
    }

    assert_eq!(unsafe {*r0_bm}, 0x38);

    layout = Layout::array::<Sample>(4).unwrap();
    let ptr3 = <Allocator as mem::Allocator<Sample>>::alloc(layout);
    assert_eq!(ptr3.as_ptr() as *const u8, unsafe {heap.add(size_of::<Sample>() * 6)});
    assert_eq!(unsafe {*r0_bm}, 0xf8);
    assert_eq!(unsafe {*r0_bm.add(1)}, 0x03);

    mem::clear_heap();
}


#[test]
fn list_alloc_test() {
    use ds::*;
    let _guard = get_test_lock().lock().unwrap();
    let mut structure: List<Sample, mem::FixedAllocator<ListNode<Sample>, {mem::Regions::Region0 as usize}>> = List::new();
    let (_, r0_bm) = mem::get_heap();
    
    structure.add_node(Sample{_a:52, _b: 12});
    structure.add_node(Sample{_a:32, _b: 13});
    structure.add_node(Sample{_a:38, _b: 1000});
    structure.add_node(Sample{_a:-12035, _b: 2});

    println!("Traversing linked list");
    let mut tmp_node = Vec::new();
    for node in structure.iter() {
        println!("{:?}", node.data);
        if node.data._a == -12035 || node.data._a == 52 {
            tmp_node.push(NonNull::new(node as *const ds::ListNode<_> as *mut ds::ListNode<_>).unwrap());
        }
    }
    assert_eq!(structure.get_nodes(), 4);
    for del_node in tmp_node {
        unsafe {
           structure.remove_node(del_node);
        }
    }
    
    println!("Traversing list after removing node._a = -12035 and 52");    
    for node in structure.iter() {
        println!("{:?}", node.data);
    }
    assert_eq!(structure.get_nodes(), 2);
    
    structure.add_node(Sample{_a:-1232, _b: 34});
    for node in structure.iter_mut() {
        node.data._a += 2;
        println!("{:?}", node.data);
    }
    assert_eq!(structure.get_nodes(), 3);
    
    for node in structure.iter() {
        println!("{:?}", node.data);
    }
    
    assert_eq!(unsafe {*r0_bm}, 0x7);
}