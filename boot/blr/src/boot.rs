#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests;

pub fn boot_main() {
    log::info!("Starting primary bootloader...");
}