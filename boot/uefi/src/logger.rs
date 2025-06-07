pub use log::{self, info, debug};
use uefi::Identify;
use uefi::boot::{self, ScopedProtocol};
use uefi::proto::console::serial::Serial;
use uefi::CString16;
use uefi::system;
use core::fmt::Write;

struct SerialLogger(Option<ScopedProtocol<Serial>>);

impl core::fmt::Write for SerialLogger {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        
        system::with_stdout(|output| {
            output.output_string(&CString16::try_from(s).unwrap()).unwrap();
        });
        
        if let Some(serial_port) = &mut self.0 {
            serial_port.write(s.as_bytes()).unwrap();
        }

        Ok(())
    }
} 

struct UefiLogger;

impl log::Log for UefiLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level() 
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {

#[allow(static_mut_refs)]
            let _ = write!(unsafe {&mut SERIAL}, "[{}]: {}\r\n", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: UefiLogger = UefiLogger{};
static mut SERIAL: SerialLogger = SerialLogger(None);


#[no_mangle]
extern "Rust" fn standard_logger() -> &'static mut dyn ::core::fmt::Write {
    unsafe {
#[allow(static_mut_refs)]
        &mut SERIAL
    }
}


pub fn init_logger() {
    system::with_stdout(|output| {
        output.clear().unwrap();
    });
    
    let mut found_serial_port = false;

    // Initialize serial port if available 
    if let Ok(supported_handles) = boot::locate_handle_buffer(boot::SearchType::ByProtocol(&Serial::GUID)) {
        let mut serial_port: ScopedProtocol<Serial> = boot::open_protocol_exclusive(*supported_handles.first().unwrap()).unwrap();
        serial_port.reset().unwrap();
        unsafe {
            SERIAL.0 = Some(serial_port);
        }        

        found_serial_port = true;
    }
    
    log::set_logger(&LOGGER).unwrap();

#[cfg(debug_assertions)]
    log::set_max_level(log::LevelFilter::Debug);

#[cfg(not(debug_assertions))]
    log::set_max_level(log::LevelFilter::Info);

    info!("Starting bootloader...");
    if found_serial_port {
        info!("Found serial port. ");
    }
    else {
        info!("Could not find serial port. Writing logs only to screen...");
    }
}