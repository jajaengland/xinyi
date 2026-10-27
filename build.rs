
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn assemble(source: &PathBuf, output: &PathBuf) {
    let status = Command::new("nasm")
        .arg("-f")
        .arg("elf32")
        .arg(source)
        .arg("-o")
        .arg(output)
        .status()
        .expect("Failed to run NASM. Is nasm installed and on PATH?");

    assert!(status.success(), "NASM failed to assemble an input file");
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Nested Cargo uses a separate target directory in build_program.py, so it
    // never waits on this kernel build's target lock. Build apps from source
    // for every kernel build as separate files for filesystem packaging.
    let python = env::var("PYTHON").unwrap_or_else(|_| "python".into());
    let status = Command::new(python)
        .arg(manifest_dir.join("tools/build_apps.py"))
        .arg("--output-dir").arg(&out_dir)
        .status().expect("Standard applications require Python 3 on PATH (or PYTHON)");
    assert!(status.success(), "Application build failed");
    for path in ["programs", "sdk", "src/window_protocol.rs", "tools/build_apps.py", "tools/build_program.py", "i686-unknown-none.json"] {
        println!("cargo:rerun-if-changed={path}");
    }

    let boot_asm = manifest_dir.join("src").join("boot.asm");
    let boot_obj = out_dir.join("boot.o");

    let interrupts_asm = manifest_dir.join("src").join("interrupts.asm");
    let interrupts_obj = out_dir.join("interrupts.o");

    let linker_script = manifest_dir.join("linker.ld");

    assemble(&boot_asm, &boot_obj);
    assemble(&interrupts_asm, &interrupts_obj);
    for name in ["disk_mbr", "disk_stage2", "demo", "bios_thunk"] {
        let source = manifest_dir.join(if name == "demo" { "programs" } else { "src" }).join(format!("{}.asm", name));
        let output = out_dir.join(format!("{}.bin", name));
        let status = Command::new("nasm").args(["-f", "bin"])
            .arg(&source).arg("-o").arg(&output).status()
            .expect("Failed to run NASM for disk loader");
        assert!(status.success(), "Disk loader assembly failed");
        println!("cargo:rerun-if-changed={}", source.display());
    }

    println!("cargo:rustc-link-arg={}", boot_obj.display());
    println!("cargo:rustc-link-arg={}", interrupts_obj.display());
    println!("cargo:rustc-link-arg=-T{}", linker_script.display());

    println!("cargo:rerun-if-changed=src/boot.asm");
    println!("cargo:rerun-if-changed=src/interrupts.asm");
    println!("cargo:rerun-if-changed=linker.ld");
}
