use common::en_flag;

struct GDT;
struct TSSDescriptor;

impl GDT {
    const L: u64 = 1 << 53;
    const P: u64 = 1 << 47;
    const DPL_SHIFT: u64 = 45;
    const ONES: u64 = 0x3 << 43;

    const fn new(long_mode: bool, present: bool, privilege: u64) -> u64 {
        // Bit 43 and 44 are always set in descriptor segments
        en_flag!(long_mode, Self::L) | en_flag!(present, Self::P) | (privilege << Self::DPL_SHIFT) | Self::ONES
    }
}

impl TSSDescriptor {
    const TSS_TYPE: u64 = 0xB;
    const TYPE_SHIFT: u64 = 40;
    const DPL: u64 = 0x3 << 45;
    const P: u64 = 1 << 47; 
    const SEG_UPPER_SHIFT: u64 = 48 - 16;
    const SEG_UPPER_MASK: u64 = 0xF << 16;
    const SEG_LOWER_MASK: u64 = 0xFFFF;
    const ADDRESS_LOWER_MASK: u64 = 0xFFFFFF;
    const ADDRESS_LOWER_SHIFT: u64 = 16;
    const ADDRESS_UPPER_MASK: u64 = 0xFFFFFFFF << 32;
    const ADDRESS_UPPER_SHIFT: u64 = 64 - 32;
    const ADDRESS_MIDDLE_MASK: u64 = 0xFF << 24;
    const ADDRESS_MIDDLE_SHIFT: u64 = 56 - 24;

    fn new(seg_limit: u64, base_address: u64) -> [u64; 2] {
        [(Self::TSS_TYPE << Self::TYPE_SHIFT) | Self::DPL | Self::P | (seg_limit & Self::SEG_LOWER_MASK) | ((seg_limit & Self::SEG_UPPER_MASK) << Self::SEG_UPPER_SHIFT)
        | ((base_address & Self::ADDRESS_LOWER_MASK) << Self::ADDRESS_LOWER_SHIFT) | ((base_address & Self::ADDRESS_MIDDLE_MASK) << Self::ADDRESS_MIDDLE_SHIFT), 
        (base_address & Self::ADDRESS_UPPER_MASK) << Self::ADDRESS_UPPER_SHIFT] 
    }
}

static GDT_DATA: [u64; 5] = [
    // GDT layout -> Null + Kernel code + Kernel data + User data + User code
    // This particular layout is required as syscall/sysret expects this
    GDT::new(false, false, 0),
    GDT::new(true, true, 0),
    GDT::new(false, true, 0),
    GDT::new(false, true, 3),
    GDT::new(true, true, 3)
];