fn main() {
    // NOTE: this is a hack to allow this crate to build on docs.rs.
    //       https://github.com/sonodima/microseh/pull/11#issuecomment-2385633164
    if std::env::var_os("CARGO_CFG_DOCSRS").is_some()
        || std::env::var_os("CARGO_CFG_WINDOWS").is_none()
    {
        println!("cargo:warning=building for a non-supported platform, exception handling will not be available");
        return;
    }

    cc::Build::new().file("src/stub.c").compile("sehstub");
}
