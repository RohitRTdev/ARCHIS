use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

fn generate_stubs(arch: &String) {
    let input = File::open(format!("src/hal/{}/asm/real.rs", arch)).expect("Assembly stub file not found");
    let reader = BufReader::new(input);

    let mut output = File::create(format!("src/hal/{}/asm/stub.rs", arch)).expect("Cannot create stub.rs");
    writeln!(output, "#![allow(unused_variables)]").unwrap();

    for line in reader.lines() {
        let mut line = line.unwrap().trim().to_string();

        if line.starts_with("pub fn") {
            line.insert_str(4, "unsafe ");
            let without_semicolon = line.trim_end_matches(';');
            writeln!(output, "{without_semicolon} {{").unwrap();

            if line.contains("!") {
                writeln!(output, "  loop{{}}").unwrap();
            }
            else if line.contains("->") {
                writeln!(output, "  0").unwrap();
            }

            writeln!(output, "}}\n").unwrap();
        }
    }
}

fn main() {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    generate_stubs(&arch);

    if Path::new("placeholder_test.txt").exists() {
        println!("cargo:warning=Skipping build.rs logic during tests.");
        return;
    }
    
    let target = format!("{}-unknown-none", arch);
    println!("cargo:rerun-if-changed=src/hal/{}/asm", arch);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let asm_dir = PathBuf::from(format!("src/hal/{}/asm", arch));
    println!("Compiling assembly files from directory: {}", asm_dir.display());

    for entry in fs::read_dir(&asm_dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "S") {
            let file_stem = path.file_stem().unwrap();
            let output_obj = out_dir.join(format!("{}.o", file_stem.to_string_lossy()));
            let input_path = path.to_str().unwrap().replace('\\', "/");
            
            let status = Command::new("clang")
                .args(&[
                    "-c",
                    "-fPIC",                   
                    "-target", &target,
                    "-I", asm_dir.to_str().unwrap(),
                    "-o", output_obj.to_str().unwrap(),
                    &input_path,
                ])
                .status()
                .expect("failed to execute assembler");

            assert!(status.success(), "Failed to assemble {}", path.display());

            // Tell cargo to link the object file
            println!("cargo:rustc-link-arg={}", output_obj.display());
        }
    }
}