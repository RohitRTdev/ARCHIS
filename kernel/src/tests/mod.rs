use crate::{ds, mem};

tests::init_test_logger!(aris);

#[test]
fn list_alloc_test() {
    struct Sample {
        _a: u32
    }
    
    let mut structure: ds::List<Sample, mem::FixedAllocator<_, {mem::Regions::Region0 as usize}>> = ds::List::new();
    structure.add_node(Sample{_a:52});
    structure.add_node(Sample{_a:35});

}