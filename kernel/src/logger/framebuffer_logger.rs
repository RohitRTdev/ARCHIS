use crate::sync::Spinlock;
use crate::{RemapEntry, BOOT_INFO, REMAP_LIST};
use common::{ceil_div, MemoryRegion};

const PSF_MAGIC: u32 = 0x864AB572;

#[repr(C)]
#[derive(Copy, Clone)]
struct PSFHeader {
    magic: u32,
    version: u32,
    headersize: u32,
    flags: u32,
    numglyph: u32,
    bytesperglyph: u32,
    height: u32,
    width: u32,
}

// Include PSF data as part of kernel binary
static FONT_DATA: &[u8] = include_bytes!("../../../resources/zap-ext-light20.psf");

pub struct FramebufferLogger {
    fb_base: *mut u8,
    width: usize,
    height: usize,
    stride: usize,
    current_x: usize,
    current_y: usize,
    font_header: PSFHeader,
    font_glyphs: *const u8
}

pub static FRAMEBUFFER_LOGGER: Spinlock<FramebufferLogger> = Spinlock::new(FramebufferLogger {
    fb_base: core::ptr::null_mut(),
    width: 0,
    height: 0,
    stride: 0,
    current_x: 0,
    current_y: 0,
    font_header: PSFHeader {
        magic: 0,
        version: 0,
        headersize: 0,
        flags: 0,
        numglyph: 0,
        bytesperglyph: 0,
        height: 0,
        width: 0,
    },
    font_glyphs: core::ptr::null()
});

impl FramebufferLogger {
    fn init(&mut self) {
        let boot_info = BOOT_INFO.get().unwrap().lock();
        let fb_info = boot_info.framebuffer_desc;
        
        self.fb_base = fb_info.fb.base_address as *mut u8;
        self.width = fb_info.width;
        self.height = fb_info.height;
        self.stride = fb_info.stride;
        
        self.load_font();
        self.clear_screen();
    }
    
    fn load_font(&mut self) {
        // Parse PSF header manually to handle endianness
        if FONT_DATA.len() < 32 {
            panic!("Font data too small");
        }
        
        // Read header fields manually (little-endian)
        self.font_header.magic = u32::from_le_bytes([
            FONT_DATA[0], FONT_DATA[1], FONT_DATA[2], FONT_DATA[3]
        ]);
        self.font_header.version = u32::from_le_bytes([
            FONT_DATA[4], FONT_DATA[5], FONT_DATA[6], FONT_DATA[7]
        ]);
        self.font_header.headersize = u32::from_le_bytes([
            FONT_DATA[8], FONT_DATA[9], FONT_DATA[10], FONT_DATA[11]
        ]);
        self.font_header.flags = u32::from_le_bytes([
            FONT_DATA[12], FONT_DATA[13], FONT_DATA[14], FONT_DATA[15]
        ]);
        self.font_header.numglyph = u32::from_le_bytes([
            FONT_DATA[16], FONT_DATA[17], FONT_DATA[18], FONT_DATA[19]
        ]);
        self.font_header.bytesperglyph = u32::from_le_bytes([
            FONT_DATA[20], FONT_DATA[21], FONT_DATA[22], FONT_DATA[23]
        ]);
        self.font_header.height = u32::from_le_bytes([
            FONT_DATA[24], FONT_DATA[25], FONT_DATA[26], FONT_DATA[27]
        ]);
        self.font_header.width = u32::from_le_bytes([
            FONT_DATA[28], FONT_DATA[29], FONT_DATA[30], FONT_DATA[31]
        ]);

        // A panic at this stage is technically not correct, since panic internally calls
        // framebuffer_logger which results in double lock (since this call already holds lock)
        // This will cause system to hang. However, since we don't want system to continue boot 
        // process if framebuffer init fails, this behaviour is fine
        if self.font_header.magic != PSF_MAGIC {
            panic!("Invalid PSF magic number: {:#X}", self.font_header.magic);
        }
        
        // Calculate glyph data offset
        let glyph_offset = self.font_header.headersize as usize;
        if glyph_offset >= FONT_DATA.len() {
            panic!("Glyph offset beyond font data");
        }

        self.font_glyphs = unsafe { FONT_DATA.as_ptr().add(glyph_offset) };
        
        // PSF v2 might store height differently - let's check the actual bytes per glyph
        let expected_height = self.font_header.bytesperglyph / ceil_div(self.font_header.width, 8);
        if expected_height != self.font_header.height {
            // Use the calculated height instead
            self.font_header.height = expected_height;
        }
    }
    
    fn get_glyph(&self, char_code: u32) -> Option<*const u8> {
        // PSF fonts typically have 256 or 512 characters starting from 0
        if char_code >= self.font_header.numglyph {
            return None;
        }
        
        let glyph_offset = (char_code * self.font_header.bytesperglyph) as usize;
        Some(unsafe { self.font_glyphs.add(glyph_offset) })
    }
    
    pub fn clear_screen(&mut self) {
        unsafe {
            self.fb_base.write_bytes(0, self.height * self.stride * 4);
        }
        
        self.current_x = 0;
        self.current_y = 0;
    }
    
    fn put_char(&mut self, c: char) {
        match c {
            '\n' => {
                self.current_x = 0;
                self.current_y += self.font_header.height as usize;
                if self.current_y >= self.height {
                    self.scroll_screen();
                }
            },
            '\r' => {
                self.current_x = 0;
            },
            '\t' => {
                let tab_width = self.font_header.width as usize * 4;
                self.current_x += tab_width - (self.current_x % tab_width);
                if self.current_x >= self.width {
                    self.current_x = 0;
                    self.current_y += self.font_header.height as usize;
                    if self.current_y >= self.height {
                        self.scroll_screen();
                    }
                }
            },
            _ => {
                self.draw_char(c);
                self.current_x += self.font_header.width as usize;
                if self.current_x >= self.width {
                    self.current_x = 0;
                    self.current_y += self.font_header.height as usize;
                    if self.current_y >= self.height {
                        self.scroll_screen();
                    }
                }
            }
        }
    }
    
    fn draw_char(&mut self, c: char) {
        let char_code = c as u32;
        
        // Try to get glyph from font
        let glyph_data = if let Some(glyph) = self.get_glyph(char_code) {
            glyph
        } else {
            // Fallback to space if character not found
            return;
        };
        
        let start_x = self.current_x;
        let start_y = self.current_y;
        let font_width = self.font_header.width;
        let font_height = self.font_header.height;
        
        // Draw the character
        for y in 0..font_height {
            for x in 0..font_width {
                let pixel_x = start_x + x as usize;
                let pixel_y = start_y + y as usize;
                
                if pixel_x < self.width && pixel_y < self.height {
                    // Calculate which byte contains this pixel
                    let bytes_per_row = ceil_div(font_width, 8); 
                    let byte_index = y * bytes_per_row + (x >> 3);
                    let bit_index = 7 - (x % 8); // PSF uses MSB first
                    
                    if byte_index < self.font_header.bytesperglyph {
                        let glyph_byte = unsafe { *glyph_data.add(byte_index as usize) };
                        let is_set = (glyph_byte & (1 << bit_index)) != 0;
                        
                        if is_set {
                            self.set_pixel(pixel_x, pixel_y, 0xFFFFFF); // White
                        } else {
                            self.set_pixel(pixel_x, pixel_y, 0x000000); // Black
                        }
                    }
                }
            }
        }
    }
    
    fn set_pixel(&mut self, x: usize, y: usize, color: u32) {
        let offset = (y * self.stride + x) * 4; // stride is in pixels, 4 bytes per pixel
        if offset < self.height * self.stride * 4 {
            unsafe {
                let pixel_ptr = self.fb_base.add(offset) as *mut u32;
                core::ptr::write_volatile(pixel_ptr, color);
            }
        }
    }
    
    fn scroll_screen(&mut self) {
        // Scroll the screen up by one line (font height only)
        let line_size = self.font_header.height as usize * self.stride * 4; // stride in pixels, *4 for bytes
        let fb_size = self.height * self.stride * 4;
        
        unsafe {
            core::ptr::copy(
                self.fb_base.add(line_size),
                self.fb_base,
                fb_size - line_size
            );
            // Clear the bottom line
            core::ptr::write_bytes(
                self.fb_base.add(fb_size - line_size),
                0,
                line_size
            );
        }
        
        self.current_y -= self.font_header.height as usize;
    }
    
    pub fn write(&mut self, s: &str) {
        for c in s.chars() {
            self.put_char(c);
        }
    }
}

impl core::fmt::Write for FramebufferLogger {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write(s);
        Ok(())
    }
}

pub fn init() {
    let mut logger = FRAMEBUFFER_LOGGER.lock();
    logger.init();
    
    REMAP_LIST.lock().add_node(RemapEntry {
        value: MemoryRegion {
            base_address: logger.fb_base as usize,
            size: logger.height * logger.stride * 4
        },
        is_identity_mapped: false
    }).unwrap();
}