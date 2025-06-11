extern crate alloc;

use alloc::format;
use alloc::collections::BTreeMap;
use log::{info, debug};
use alloc::vec;
use alloc::{string::String, vec::Vec};
use uefi::proto::device_path::text::{AllowShortcuts, DisplayOnly};
use uefi::proto::media::file::{File, FileAttribute, RegularFile};
use uefi::{boot, CString16, Char16, Handle};
use uefi::boot::{ScopedProtocol, HandleBuffer};
use uefi::proto::{device_path::{DevicePath, media::{self,HardDrive}}, media::{file::FileMode, fs::SimpleFileSystem}};

const ROOT_GUID: &str = "9ffd2959-915c-479f-8787-1f9f701e1034";  

pub struct FileTable {
    filetable: BTreeMap<String, Vec<u8>>
}

impl FileTable {
    pub fn fetch_file_data(&self, filename: String) -> &[u8] {
        let val = self.filetable.get(&filename);
        assert!(val.is_some());
        val.unwrap()
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

pub fn load_init_fs(root: &Handle, files: &[&str]) -> FileTable {
    // Load all boot start drivers, font file, kernel elf etc
    let mut boot_fs: ScopedProtocol<SimpleFileSystem> = boot::open_protocol_exclusive(*root).expect("Could not open file protocol on root partition"); 

    let mut dir = boot_fs.open_volume().expect("Could not open root partition");
    let mut map = BTreeMap::new();
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
        map.insert(String::from(*filename), buf);
    }

    FileTable {filetable: map}
}