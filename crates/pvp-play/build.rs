fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu")
    {
        // Miniquad links the unprefixed imm32 library. The winapi dependency
        // already ships the same import library as libwinapi_imm32.a, while
        // Rust's minimal bundled MinGW omits libimm32.a. GNU ld accepts this
        // tiny linker script as an archive-name forwarder; no SDK installation
        // or machine-specific registry path is needed.
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        std::fs::write(out.join("libimm32.a"), "INPUT(-lwinapi_imm32)\n").unwrap();
        println!("cargo:rustc-link-search=native={}", out.display());
    }
}
