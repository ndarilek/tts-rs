fn main() {
    if std::env::var("TARGET").unwrap().contains("-apple") {
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        if !std::env::var("CARGO_CFG_TARGET_OS")
            .unwrap()
            .contains("ios")
        {
            println!("cargo:rustc-link-lib=framework=AppKit");
        }
    }
}
