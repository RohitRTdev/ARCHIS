extern crate alloc;

use log::info;
use alloc::string::String;
use uefi::proto::device_path::text::{AllowShortcuts, DisplayOnly};
use uefi::{Identify, boot};
use uefi::boot::ScopedProtocol;
use uefi::proto::{device_path::{DevicePath, media::{self,HardDrive}}, media::fs::SimpleFileSystem};

const ROOT_GUID: &str = "9ffd2959-915c-479f-8787-1f9f701e1034";  

pub fn list_fs() {
    // First get all available handles for partition information protocol
    info!("Fetching FAT32 formatted partitions...");
    let supported_handles = boot::locate_handle_buffer(boot::SearchType::ByProtocol(&SimpleFileSystem::GUID)).unwrap();

    let mut found_partition = false;
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
                        info!("Found root parition with guid:{}", ROOT_GUID);
                        found_partition = true;
                    }
                }           
            }
        }
    }

    if !found_partition {
        info!("Could not find root partition...");
    }

}