#![allow(dead_code)]

use std::path::PathBuf;

use bindgen;
use ewdk::*;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    gen_bindings("dxgkrnl");
    gen_bindings("wdm");
    gen_bindings("wdf");
}

fn gen_bindings(header: &str) {
    let out_path = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    let bindgen = ewdk_bindgen()
        .clang_arg(format!("-I{}", out_path.to_str().unwrap()))
        .header(format!("src/{}.h", header))
        .use_core()
        .rust_target(bindgen::RustTarget::nightly())
        .rust_edition(bindgen::RustEdition::Edition2024)
        .wrap_unsafe_ops(true)
        .derive_debug(false)
        .layout_tests(false)
        .ctypes_prefix("::core::ffi")
        .default_enum_style(bindgen::EnumVariation::NewType {
            is_bitfield: false,
            is_global: false,
        })
        .blocklist_type("_?P?KPCR.*")
        .blocklist_type("_?P?KIDTENTRY64")
        .blocklist_type("_?P?KGDTENTRY64")
        .blocklist_type("P?C?NTSTATUS")
        .blocklist_function("memcmp")
        .blocklist_function("memcpy")
        .blocklist_function("memset")
        .blocklist_function("strlen")
        .blocklist_function("memmove");

    bindgen
        .generate()
        .unwrap()
        .write_to_file(out_path.join(format!("{}-bindings.rs", header)))
        .unwrap();
}
