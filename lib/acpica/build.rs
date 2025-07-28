use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

fn main() {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let acpica_src = Path::new("acpica_c");

    // Recursively collect all .c files
    let mut c_files = Vec::new();
    for entry in WalkDir::new(acpica_src)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "c").unwrap_or(false))
    {
        println!("cargo:rerun-if-changed={}", entry.path().display());
        c_files.push(entry.path().to_owned());
    }

    let mut obj_files = Vec::new();
    let target = format!("{}-unknown-none", arch);

    for file in &c_files {
        let file_stem = file.file_stem().unwrap().to_string_lossy();
        let obj_path = out_dir.join(format!("{}.o", file_stem));

        let status = Command::new("clang")
            .args(&[
                "-c",
                "-fPIC",
                "-O2",
                "-msoft-float",
                "-mno-sse",
                "-mno-sse2",
                "-mno-avx",
                "-mno-mmx",
                "-fno-tree-vectorize",
                "-fno-vectorize",
                "-fno-slp-vectorize",
                "-fno-builtin",
                "-nostdlib",
                "-ffreestanding",
                "-fno-lto",
                "-Iacpica_c/include",
                "-target", &target,
                file.to_str().unwrap(),
                "-o",
                obj_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run clang");

        if !status.success() {
            panic!("Failed to compile {:?}", file);
        }

        obj_files.push(obj_path);
    }

    // Archive object files into libacpica.a using ar
    let staticlib_path = out_dir.join("libacpica.a");
    let mut ar_cmd = Command::new("llvm-ar");
    ar_cmd.arg("crs").arg(&staticlib_path);
    for obj in &obj_files {
        ar_cmd.arg(obj);
    }

    let status = ar_cmd.status().expect("Failed to run ar");
    if !status.success() {
        panic!("Failed to archive object files into libacpica.a");
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=acpica");
}
