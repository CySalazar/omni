// bootloader 0.9 requires the kernel ELF to be ET_EXEC (static executable).
// Rust 1.83+ sets position_independent_executables = true for x86_64-unknown-none,
// which causes rustc to pass -pie to LLD, producing ET_DYN.
//
// Build script cargo:rustc-link-arg outputs are placed AFTER the target-spec
// flags in the linker command, so --no-pie here unconditionally overrides the
// target spec's -pie (last flag wins in LLD).
//
// `println!` here is the Cargo-prescribed mechanism for emitting build-script
// directives; there is no alternative. The workspace `disallowed_macros` rule
// targets application/library code, not build scripts, so we allow it here.
#![allow(clippy::disallowed_macros)]
fn main() {
    println!("cargo:rustc-link-arg=--no-pie");
}
