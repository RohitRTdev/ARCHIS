use crate::sync::Spinlock;
use crate::hal;
pub static SERIAL: Spinlock<SerialLogger> = Spinlock::new(SerialLogger(false));

pub struct SerialLogger(bool);

#[cfg(target_arch="x86_64")]
const SERIAL_PORT: u16 = 0x3F8;


#[cfg(target_arch="x86_64")]
impl SerialLogger {
    fn serial_write_byte(&self, byte: u8) {
        unsafe {
            while (hal::read_port_u8(SERIAL_PORT + 5) & 0x20) == 0 {} // Wait for transmit buffer to be empty
            hal::write_port_u8(SERIAL_PORT, byte);
        }
    }
    
    pub fn write(&self, s: &str) {
        // Serial port is not available
        if !self.0 {
            return;
        }
        
        for b in s.bytes() {
            self.serial_write_byte(b);
        }
    }
}

#[cfg(target_arch="x86_64")]
pub fn init() {
    unsafe {
        hal::write_port_u8(SERIAL_PORT + 1, 0x00); // Disable all interrupts
        hal::write_port_u8(SERIAL_PORT + 3, 0x80); // Enable DLAB (set baud rate divisor)
        hal::write_port_u8(SERIAL_PORT + 0, 0x03); // Set divisor to 3 (low byte) 38400 baud
        hal::write_port_u8(SERIAL_PORT + 1, 0x00); // (high byte)
        hal::write_port_u8(SERIAL_PORT + 3, 0x03); // 8 bits, no parity, one stop bit
        hal::write_port_u8(SERIAL_PORT + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
        hal::write_port_u8(SERIAL_PORT + 4, 0x17); // IRQs disabled, RTS/DSR set and enable loopback

        // Loopback test. Check if we have a serial port or not
        hal::write_port_u8(SERIAL_PORT + 0, 0xAE);  // Test serial chip (send byte 0xAE and check if serial returns same byte)

        if hal::read_port_u8(SERIAL_PORT + 0) == 0xAE {
            SERIAL.lock().0 = true;
        }

        // If serial is not faulty set it in normal operation mode
        // (not-loopback with IRQs disabled and OUT#1 and OUT#2 bits enabled)
        hal::write_port_u8(SERIAL_PORT + 4, 0x07);
    }
}