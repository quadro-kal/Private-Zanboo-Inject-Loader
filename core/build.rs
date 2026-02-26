// ========================================================
// ZIL FRAMEWORK: BUILD SCRIPT (build.rs)
// Dijalankan oleh Cargo SEBELUM kompilasi Rust dimulai.
// Tugasnya: menghasilkan binding FFI antara C dan Rust.
// ========================================================

fn main() {
    // --- 1. BERITAHU CARGO PATH LIBRARY C/ASSEMBLY ---
    // Cargo perlu tahu di mana mencari file .a yang akan di-link
    println!("cargo:rustc-link-search=native=../build/obj");
    
    // --- 2. LINK FILE OBJEK C/ASSEMBLY ---
    // Ini memerintahkan linker untuk menyertakan objek ini
    println!("cargo:rustc-link-lib=static=zil_arch");

    // --- 3. RERUN JIKA FILE C/ASM BERUBAH ---
    // Tanpa ini, perubahan di file C tidak akan trigger rebuild Rust
    println!("cargo:rerun-if-changed=../arch/arm64/boot.s");
    println!("cargo:rerun-if-changed=../arch/arm64/pac_core.s");
    println!("cargo:rerun-if-changed=../arch/arm64/pac_wrapper.c");
    println!("cargo:rerun-if-changed=../arch/arm64/mmu.c");
    println!("cargo:rerun-if-changed=../driver/iokit_shim.c");
    println!("cargo:rerun-if-changed=../include/shared_types.h");
    println!("cargo:rerun-if-changed=../include/pac_defs.h");
    println!("cargo:rerun-if-changed=../include/zil_memory_map.h");

    // --- 4. TARGET PLATFORM VALIDATION ---
    // Pastikan hanya dikompilasi untuk iOS/macOS ARM64
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("aarch64") {
        panic!(
            "ZIL hanya mendukung target aarch64 (ARM64). \
             Target saat ini: {}. \
             Gunakan: cargo build --target aarch64-apple-ios",
            target
        );
    }
}
