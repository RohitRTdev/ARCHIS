extern crate alloc;

use core::alloc::Layout;
use core::ptr::copy_nonoverlapping;

use alloc::format;
use log::info;
use alloc::vec;
use alloc::{string::String, vec::Vec};
use uefi::proto::device_path::text::{AllowShortcuts, DisplayOnly};
use uefi::proto::media::file::{File, FileAttribute, RegularFile};
use uefi::{boot, CString16, Char16, Handle};
use uefi::boot::{ScopedProtocol, HandleBuffer};
use uefi::proto::{device_path::{DevicePath, media::{self,HardDrive}}, media::{file::FileMode, fs::SimpleFileSystem}};
use common::{FileDescriptor, PAGE_SIZE};
use crate::loader_alloc;

const ROOT_GUID: &str = "9ffd2959-915c-479f-8787-1f9f701e1034";  


pub struct FileTable<'a> {
    pub descriptors: &'a mut[FileDescriptor<'a>],
    capacity: usize,
    pub length: usize
}

impl<'a> FileTable<'a> {
    fn new() -> Self {
        // Let's start with a backing memory of 1 page size
        // We're not simply creating a vector here as we need precise control over alignment of memory
        let init_cap = PAGE_SIZE;
        let length = init_cap / size_of::<FileDescriptor>();
        let layout = Layout::from_size_align(init_cap, PAGE_SIZE).unwrap();
        Self {
            descriptors: unsafe {
                    core::slice::from_raw_parts_mut(loader_alloc(layout) as *mut FileDescriptor, length)
            },
            capacity: init_cap,
            length: 0
        }
    }

    fn insert(&mut self, name: &str, value: &[u8]) {
        let layout = Layout::from_size_align(name.len() + value.len(), PAGE_SIZE).unwrap();
        let loc = loader_alloc(layout);

        // Copy the file contents first before name
        // This ensures elf file is 4K aligned. String can be arbitrary alignment
        unsafe {
            copy_nonoverlapping(value.as_ptr(), loc, value.len());
            copy_nonoverlapping(name.as_ptr(), loc.add(value.len()), name.len());
        }

        let desc = FileDescriptor {
            name: unsafe {
                core::str::from_utf8(core::slice::from_raw_parts(loc.add(value.len()), name.len())).unwrap()
            },
            contents: unsafe {
                core::slice::from_raw_parts(loc, value.len())
            }
        };

        // TODO: If not enough capacity, then allocate bigger array and copy old contents to it
        // For now, this should suffice
        assert!(self.length < self.capacity / size_of::<FileDescriptor>());
        self.descriptors[self.length] = desc;
        self.length += 1;
    } 
    
    pub fn fetch_file_data(&self, filename: &str) -> Option<&[u8]> {
        for desc in self.descriptors.iter() {
            if desc.name == filename {
                return Some(desc.contents)
            }
        }
    
        None
    }
}


pub fn list_fs(supported_handles: &HandleBuffer) -> &Handle {
    let mut root_partition = None;
    for partition in supported_handles.iter() {
        // For each handle, get it's devicePath
        let device_path: ScopedProtocol<DevicePath> = boot::open_protocol_exclusive(*partition).unwrap();
        
        if let Ok(device_path_text) = device_path.to_string(DisplayOnly(false), AllowShortcuts(true)) {
            info!("Device path = {}", String::from(&device_path_text));
        }

        // Iterate the device path and check for our root partition id
        for device_node in device_path.node_iter() {
            if let Ok(device_node_data) = <&HardDrive>::try_from(device_node) {
                if let media::PartitionSignature::Guid(guid) = device_node_data.partition_signature() {
                    let ascii_data = guid.to_ascii_hex_lower();
                    if str::from_utf8(&ascii_data).unwrap() == ROOT_GUID {
                        info!("Found root partition with guid:{}", ROOT_GUID);
                        root_partition = Some(partition);
                        break;
                    }
                }           
            }
        }
    }

    if root_partition.is_none() {
        panic!("Could not find root partition!!");
    }

    root_partition.unwrap()
}

pub fn load_init_fs<'a>(root: &Handle, files: &[&str]) -> FileTable<'a> {
    // Load all boot start drivers, font file, kernel elf etc
    let mut boot_fs: ScopedProtocol<SimpleFileSystem> = boot::open_protocol_exclusive(*root).expect("Could not open file protocol on root partition"); 

    let mut dir = boot_fs.open_volume().expect("Could not open root partition");
    let mut map = FileTable::new();
    for filename in files {
        let mut filename_dos = CString16::try_from(*filename).unwrap();
        filename_dos.replace_char(Char16::try_from('/').unwrap(), Char16::try_from('\\').unwrap());

        let file = dir.open(&filename_dos, FileMode::Read, FileAttribute::READ_ONLY).
        expect(format!("Could not open file={}", filename).as_str());
        
        let mut reg_file = file.into_regular_file().unwrap();
        reg_file.set_position(RegularFile::END_OF_FILE).unwrap();
        let file_size = reg_file.get_position().unwrap();
        reg_file.set_position(0).unwrap();

        let mut buf: Vec<u8> = vec![0; file_size as usize];
        reg_file.read(buf.as_mut_slice()).unwrap();

        info!("Loaded file={} of size={} at location={:#X}", filename, file_size, buf.as_ptr() as usize);
        map.insert(*filename, buf.as_slice());
    }

    map
}