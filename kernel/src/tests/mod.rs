use crate::{ds, mem};

tests::init_test_logger!(aris);

struct Sample {
    _a: u32,
    _b: u32
}

#[test]
fn fixed_allocator_test() {
    type Allocator = mem::FixedAllocator<Sample, {mem::Regions::Region0 as usize}>;
    
    use core::alloc::Layout;
    let mut layout = Layout::array::<Sample>(3).unwrap();   

    let (heap, r0_bm) = mem::get_heap();

    // This should allocate first 3 slots in heap from region 0
    let ptr = <Allocator as mem::Allocator<Sample>>::alloc(layout);
    assert_eq!(ptr.as_ptr() as *const u8, heap);

    <Allocator as mem::Allocator<Sample>>::alloc(layout);

}


#[test]
fn list_alloc_test() {
    
    let mut structure: ds::List<Sample, mem::FixedAllocator<_, {mem::Regions::Region0 as usize}>> = ds::List::new();
    structure.add_node(Sample{_a:52, _b: 12});
    structure.add_node(Sample{_a:35, _b: 13});

}