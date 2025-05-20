use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target = format!("{}-unknown-none", arch);
    println!("cargo:rerun-if-changed=src/hal/{}/asm", arch);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let asm_dir = PathBuf::from(format!("src/hal/{}/asm", arch));
    println!("Compiling assembly files from directory: {}", asm_dir.display());

    for entry in fs::read_dir(&asm_dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "s") {
            let file_stem = path.file_stem().unwrap();
            let output_obj = out_dir.join(format!("{}.o", file_stem.to_string_lossy()));

            let status = Command::new("clang")
                .args(&[
                    "-c",                        
                    "-target", &target,          
                    "-o", output_obj.to_str().unwrap(),
                    path.to_str().unwrap(),
                ])
                .status()
                .expect("failed to execute assembler");

            assert!(status.success(), "Failed to assemble {}", path.display());

            // Tell cargo to link the object file
            println!("cargo:rustc-link-arg={}", output_obj.display());
        }
    }
}