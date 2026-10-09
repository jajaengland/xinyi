
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").unwrap()
    );

    let out_dir = PathBuf::from(
        env::var("OUT_DIR").unwrap()
    );

    let boot_asm = manifest_dir.join("src").join("boot.asm");
    let boot_obj = out_dir.join("boot.o");
    let linker_script = manifest_dir.join("linker.ld");

    let status = Command::new("nasm")
        .arg("-f")
        .arg("elf32")
        .arg(&boot_asm)
        .arg("-o")
        .arg(&boot_obj)
        .status()
        .expect("Failed to run NASM. Is nasm installed and on PATH?");

    assert!(status.success(), "NASM failed to assemble boot.asm");

    println!("cargo:rustc-link-arg={}", boot_obj.display());
    println!(
        "cargo:rustc-link-arg=-T{}",
        linker_script.display()
    );
    println!("cargo:rerun-if-changed=src/boot.asm");
    println!("cargo:rerun-if-changed=linker.ld");
}
