//! OMNI OS UEFI/BIOS disk image builder.
//!
//! Wraps the `kernel-runner` ELF into bootable disk images using
//! `bootloader 0.11`'s `UefiBoot` and `BiosBoot` builders.
//!
//! # Usage
//!
//! ```text
//! cargo run --manifest-path disk-image/Cargo.toml -- <kernel-elf-path>
//! ```
//!
//! The tool creates two files in the same directory as the kernel ELF:
//! - `boot-uefi.img` — UEFI bootable (GPT + FAT, requires OVMF in QEMU)
//! - `boot-bios.img` — BIOS bootable (MBR, SeaBIOS)
//!
//! Both paths are printed to stdout as `UEFI:<path>` and `BIOS:<path>`.

use std::path::PathBuf;
use bootloader::{UefiBoot, BiosBoot};

fn main() {
    let kernel_path: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("usage: disk-image <kernel-elf-path>");
            std::process::exit(1);
        });

    if !kernel_path.exists() {
        eprintln!("error: kernel ELF not found: {}", kernel_path.display());
        std::process::exit(1);
    }

    let out_dir = kernel_path
        .parent()
        .expect("kernel ELF path must have a parent directory");

    // ── UEFI image ────────────────────────────────────────────────────────────
    let uefi_out = out_dir.join("boot-uefi.img");
    UefiBoot::new(&kernel_path)
        .create_disk_image(&uefi_out)
        .unwrap_or_else(|e| {
            eprintln!("error: failed to create UEFI disk image: {e}");
            std::process::exit(1);
        });
    println!("UEFI:{}", uefi_out.display());

    // ── BIOS image (fallback) ─────────────────────────────────────────────────
    let bios_out = out_dir.join("boot-bios.img");
    BiosBoot::new(&kernel_path)
        .create_disk_image(&bios_out)
        .unwrap_or_else(|e| {
            eprintln!("error: failed to create BIOS disk image: {e}");
            std::process::exit(1);
        });
    println!("BIOS:{}", bios_out.display());
}
