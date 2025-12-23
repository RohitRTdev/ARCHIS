pub const X2APIC_NMI: u8 = 0xA;
pub const XAPIC_NMI: u8 = 0x4;
pub const XLAPIC: u8 = 0x0;
pub const X2LAPIC: u8 = 0x9;

pub const IOAPIC_VER_OFFSET: u32 = 0x1;
pub const IOAPIC_REDIR_START_OFFSET: u32 = 0x10;
pub const MADT_TYPE_IOAPIC: u8 = 1;
pub const INT_SRC_OVERRIDE: u8 = 2;

#[repr(C, packed)]
pub struct MadtEntryHeader {
    pub entry_type: u8,
    pub length: u8,
}

#[repr(C, packed)]
pub struct IoapicEntry {
    pub header: MadtEntryHeader,
    pub id: u8,
    pub res: u8,
    pub addr: u32,
    pub gsi: u32
}

#[repr(C, packed)]
pub struct IntEntry {
    pub header: MadtEntryHeader,
    pub bus: u8,
    pub src: u8,
    pub gsi: u32,
    pub flags: u16
}

#[repr(C, packed)]
pub struct MadtLapic {
    pub hdr: MadtEntryHeader,
    pub uid: u8,
    pub apic_id: u8,
    pub flags: u32
}

#[repr(C, packed)]
pub struct MadtX2Lapic {
    pub hdr: MadtEntryHeader,
    pub res: u16,
    pub apic_id: u32,
    pub flags: u32,
    pub uid: u32
}

#[repr(C, packed)]
pub struct MadtLapicNmi {
    pub hdr: MadtEntryHeader,
    pub uid: u8,
    pub flags: u16,
    pub pin: u8
}

#[repr(C, packed)]
pub struct MadtX2LapicNmi {
    pub hdr: MadtEntryHeader,
    pub flags: u16,
    pub uid: u32,
    pub pin: u8,
    pub res: [u8; 3]
}
