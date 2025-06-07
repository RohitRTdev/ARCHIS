extern crate alloc;

use common::println;
use log::{info, debug};
use alloc::vec;
use alloc::{string::String, vec::Vec};
use uefi::proto::device_path::text::{AllowShortcuts, DisplayOnly};
use uefi::proto::media::file::{File, FileAttribute, RegularFile};
use uefi::{Handle, boot, CString16};
use uefi::boot::{ScopedProtocol, HandleBuffer};
use uefi::proto::{device_path::{DevicePath, media::{self,HardDrive}}, media::{file::FileMode, fs::SimpleFileSystem}};

const ROOT_GUID: &str = "9ffd2959-915c-479f-8787-1f9f701e1034";  

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

pub fn load_init_fs(root: &Handle) {
    // Load all boot start drivers, font file, kernel elf etc
    let mut boot_fs: ScopedProtocol<SimpleFileSystem> = boot::open_protocol_exclusive(*root).expect("Could not open file protocol on root partition"); 

    let mut dir = boot_fs.open_volume().expect("Could not open root partition");

    let file = dir.open(&CString16::try_from("sys\\aris.elf").unwrap(), FileMode::Read, FileAttribute::READ_ONLY).expect("Could not kernel file");

    let mut reg_file = file.into_regular_file().unwrap();
    reg_file.set_position(RegularFile::END_OF_FILE).unwrap();
    let file_size = reg_file.get_position().unwrap();
    reg_file.set_position(0).unwrap();

    let mut buf: Vec<u8> = vec![0; file_size as usize];

    reg_file.read(buf.as_mut_slice()).unwrap();

    let kinfo = blr::load_kernel(buf.as_ptr());
    debug!("{:?}", kinfo);

}