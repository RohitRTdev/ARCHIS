use common::{FBInfo, MemoryRegion, PixelMask};
use uefi::{boot, Identify};
use uefi::boot::ScopedProtocol;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat::*};

pub fn get_primary_gpu_framebuffer() -> FBInfo {
    let supported_handles = boot::locate_handle_buffer(boot::SearchType::ByProtocol(&GraphicsOutput::GUID)).expect("No compatible GPU found!");


    let mut gpu: ScopedProtocol<GraphicsOutput> = boot::open_protocol_exclusive(*supported_handles.first().unwrap()).unwrap();
    let cur_mode = gpu.current_mode_info();
    
    let (red_mask, blue_mask, green_mask, alpha_mask) = match cur_mode.pixel_format() {
        Rgb => {
            (0xff, 0xff0000, 0xff00, 0xff000000)
        },
        Bgr => {
            (0xff0000, 0xff, 0xff00, 0xff000000)
        }
        Bitmask => {
            let mask = cur_mode.pixel_bitmask().unwrap();
            (mask.red, mask.blue, mask.green, mask.reserved)
        }
        BltOnly => {
            panic!("Primary GPU not compatible as direct framebuffer access not allowed!");
        }
    };

    FBInfo {
        fb: MemoryRegion {
            base_address: gpu.frame_buffer().as_mut_ptr() as usize,
            size: gpu.frame_buffer().size()
        },
        height: cur_mode.resolution().1,
        width: cur_mode.resolution().0,
        stride: cur_mode.stride(),
        pixel_mask: PixelMask {
            red_mask,
            blue_mask,
            green_mask,
            alpha_mask
        }
    }
       
}