#![allow(unused_imports)]

use std::{env, process::Command};

#[derive(Debug)]
pub struct EwdkConfig {
    pub vc_tools_dir: String,
    pub win_sdk_dir: String,
    pub win_sdk_version: String,
    pub vfs_overlay: String,
    pub arch: String,
}

impl EwdkConfig {
    pub fn new(vc_tools_dir: &str, win_sdk_dir: &str, win_sdk_version: &str, vfs_overlay: &str, arch: &str) -> Self {
        Self {
            vc_tools_dir: vc_tools_dir.to_string(),
            win_sdk_dir: win_sdk_dir.to_string(),
            win_sdk_version: win_sdk_version.to_string(),
            vfs_overlay: vfs_overlay.to_string(),
            arch: arch.to_string(),
        }
    }

    pub fn win_sdk_dir(&self, vfs: bool) -> &str {
        if vfs {
            "/winsys/sdk"
        } else {
            &self.win_sdk_dir
        }
    }

    pub fn include_path(&self, mode: &str, vfs: bool) -> String {
        format!("{}/{}/{}/{}", self.win_sdk_dir(vfs), "Include", self.win_sdk_version, mode)
    }

    pub fn shared_include_path(&self, vfs: bool) -> String {
        self.include_path("shared", vfs)
    }

    pub fn kernel_include_path(&self, vfs: bool) -> String {
        self.include_path("km", vfs)
    }

    pub fn wdf_include_path(&self, vfs: bool) -> String {
        format!("{}/{}", self.win_sdk_dir(vfs), "Include")
    }

    pub fn user_include_path(&self, vfs: bool) -> String {
        self.include_path("um", vfs)
    }

    pub fn kernel_lib_path(&self, vfs: bool) -> String {
        format!("{}/{}/{}/{}/{}", self.win_sdk_dir(vfs), "Lib", self.win_sdk_version, "km", self.arch)
    }

    pub fn wdf_lib_path(&self, vfs: bool) -> String {
        const WDF_VERSION: &'static str = "1.27";
        format!("{}/{}/{}/{}/{}/{}", self.win_sdk_dir(vfs), "Lib", "wdf", "kmdf", self.arch, WDF_VERSION)
    }

    pub fn kernel_arch_define(&self) -> &'static str {
        match self.arch.as_str() {
            "x64" => "_AMD64_",
            "arm64" => "_ARM64_",
            _ => unreachable!(),
        }
    }
}

pub fn ewdk() -> EwdkConfig {
    let target = std::env::var("TARGET").unwrap();

    let arch = if target.contains("x86_64") {
        "x64"
    } else if target.contains("aarch64") {
        "arm64"
    } else {
        panic!("The target {target} is currently not supported.");
    };

    assert!(target.contains("-windows-msvc"));

    let lines: Vec<_> = Command::new("bash")
        .args(["-c", "source ../ewdk.env; source ../../ewdk.env; echo $VCTOOLSDIR; echo $WINSDKDIR; echo $WINSDKVER; echo $VFSOVERLAY"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|stdout| Some(stdout.split("\n").map(String::from).filter(|s| !s.is_empty()).collect()))
        .unwrap();

    let vc_tools_dir = lines.get(0).unwrap();
    let win_sdk_dir = lines.get(1).unwrap();
    let win_sdk_version = lines.get(2).unwrap();
    let vfs_overlay = lines.get(3).unwrap();

    let ewdk_config = EwdkConfig::new(vc_tools_dir, win_sdk_dir, win_sdk_version, vfs_overlay, arch);

    println!("EWDK Config: {:?}", ewdk_config);

    println!("cargo:rustc-link-search=native={}", ewdk_config.kernel_lib_path(false));
    println!("cargo:rustc-link-search=native={}", ewdk_config.wdf_lib_path(false));

    ewdk_config
}

const DXGKDDI_INTERFACE_VERSION: &'static str = "DXGKDDI_INTERFACE_VERSION_WDDM2_0";

pub fn ewdk_cc() -> cc::Build {
    let ewdk_config = ewdk();

    let kernel_include_dir = ewdk_config.kernel_include_path(true);
    let shared_include_dir = ewdk_config.shared_include_path(true);
    let kernel_arch_define = ewdk_config.kernel_arch_define();

    let mut build = cc::Build::new();
    build
        .compiler("clang-cl")
        .flag("-Wno-nonportable-include-path")
        .flag("-Wno-ignored-attributes")
        .flag("-Wno-ignored-pragma-intrinsic")
        .flag("-Wno-deprecated-declarations")
        .flag("-Wno-pragma-pack")
        .flag("-Wno-unknown-pragmas")
        .flag("-Wno-unused-local-typedef")
        .flag("-Wno-microsoft-anon-tag")
        .flag("-Wno-macro-redefined")
        .flag("-Wno-microsoft-enum-forward-reference")
        .flag("-Wno-unused-but-set-variable")
        .flag("-Wno-visibility")
        .flag("-Wno-unused-value")
        .flag("-Wno-extra-tokens")
        .flag("-Wno-invalid-token-paste")
        .flag("-fms-compatibility")
        .flag("-fms-extensions")
        .flag(format!("-D{}={}", "DXGKDDI_INTERFACE_VERSION", DXGKDDI_INTERFACE_VERSION))
        .flag(format!("-D{}", kernel_arch_define))
        .flag(format!("-I{}", shared_include_dir))
        .flag(format!("-I{}", kernel_include_dir))
        .flag(format!("-I{}/crt", kernel_include_dir))
        .flag(format!("-vfsoverlay{}", &ewdk_config.vfs_overlay));

    build
}

pub fn ewdk_bindgen() -> bindgen::Builder {
    let ewdk_config = ewdk();

    let wdf_include_dir = ewdk_config.wdf_include_path(true);
    let kernel_include_dir = ewdk_config.kernel_include_path(true);
    let shared_include_dir = ewdk_config.shared_include_path(true);
    let kernel_arch_define = ewdk_config.kernel_arch_define();

    //let mut bindgen = bindgen::Builder::default();

    bindgen::Builder::default()
        .clang_arg("-Wno-nonportable-include-path")
        .clang_arg("-Wno-ignored-attributes")
        .clang_arg("-Wno-ignored-pragma-intrinsic")
        .clang_arg("-Wno-deprecated-declarations")
        .clang_arg("-Wno-pragma-pack")
        .clang_arg("-Wno-visibility")
        .clang_arg("-Wno-unused-value")
        .clang_arg("-Wno-extra-tokens")
        .clang_arg("-Wno-invalid-token-paste")
        .clang_arg("-fms-compatibility")
        .clang_arg("-fms-extensions")
        .clang_arg(format!("-D{}={}", "DXGKDDI_INTERFACE_VERSION", DXGKDDI_INTERFACE_VERSION))
        .clang_arg(format!("-D{}", kernel_arch_define))
        .clang_arg(format!("-I{}", shared_include_dir))
        .clang_arg(format!("-I{}", kernel_include_dir))
        .clang_arg(format!("-I{}", wdf_include_dir))
        //.clang_arg(format!("-I{}", user_include_dir))
        .clang_arg(format!("-I{}/crt", kernel_include_dir))
        .clang_arg(format!("-vfsoverlay{}", &ewdk_config.vfs_overlay))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
}
