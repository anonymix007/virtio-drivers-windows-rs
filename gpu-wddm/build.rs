use std::{env, process::Command};

use chrono::Utc;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    set_build_version();

    println!("cargo:link-arg=/ENTRY:driver_entry");
}

fn git_hash() -> Option<String> {
    let commit_hash = Command::new("git")
        .args(["rev-parse", "--short=10", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let commit_hash = String::from_utf8(output.stdout)
                .ok()
                .map(|s| s.trim().to_string());

            commit_hash
        })?;

    let is_dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map(|out| !out.stdout.is_empty())
        .unwrap_or(false);

    Some(if is_dirty { format!("{}-dirty", commit_hash) } else { commit_hash })
}

fn set_build_version() {
    let build_date = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");

    let mut version = env!("CARGO_PKG_VERSION").to_string();
    if let Some(git) = git_hash() {
        version.push_str(&format!("-{git} "));
    }

    println!("cargo:rustc-env=BUILD_VERSION={}", version);
    println!("cargo:rustc-env=BUILD_DATE={}", build_date);
}
