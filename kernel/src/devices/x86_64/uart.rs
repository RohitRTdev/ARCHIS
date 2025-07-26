use crate::hal::{write_port_u8, read_port_u8};
use crate::sync::Spinlock;

const SERIAL_PORT: u16 = 0x3F8;
pub struct Uart {
    is_available: bool
}

impl Uart {
    fn serial_write_byte(byte: u8) {
        unsafe {
            while (read_port_u8(SERIAL_PORT + 5) & 0x20) == 0 {} // Wait for transmit buffer to be empty
            write_port_u8(SERIAL_PORT, byte);
        }
    }

    #[inline(always)]
    pub fn write(&self, s: &str) {
        if !self.is_available {
            return;
        }
        
        for b in s.bytes() {
            Self::serial_write_byte(b);
        }
    }
}

pub static SERIAL: Spinlock<Uart> = Spinlock::new(Uart { is_available: false });

pub fn init() {
    unsafe {
        write_port_u8(SERIAL_PORT + 1, 0x00); // Disable all interrupts
        write_port_u8(SERIAL_PORT + 3, 0x80); // Enable DLAB (set baud rate divisor)
        write_port_u8(SERIAL_PORT + 0, 0x03); // Set divisor to 3 (low byte) 38400 baud
        write_port_u8(SERIAL_PORT + 1, 0x00); // (high byte)
        write_port_u8(SERIAL_PORT + 3, 0x03); // 8 bits, no parity, one stop bit
        write_port_u8(SERIAL_PORT + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
        write_port_u8(SERIAL_PORT + 4, 0x17); // IRQs disabled, RTS/DSR set and enable loopback

        // Loopback test. Check if we have a serial port or not
        write_port_u8(SERIAL_PORT + 0, 0xAE);  // Test serial chip (send byte 0xAE and check if serial returns same byte)

        if read_port_u8(SERIAL_PORT + 0) == 0xAE {
            SERIAL.lock().is_available = true;
        }

        // If serial is not faulty set it in normal operation mode
        // (not-loopback with IRQs disabled and OUT#1 and OUT#2 bits enabled)
        write_port_u8(SERIAL_PORT + 4, 0x07);
    }
}
