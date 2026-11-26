//! iOS simulator builds embed the entitlements Xcode would add at link time.
//! Without an application identifier the simulator refuses Keychain access,
//! and the personal vault keeps its wrapping key there. Device builds get
//! their entitlements from the provisioning profile when they are signed.

/// Matches `bundle.identifier` in Dioxus.toml.
const BUNDLE_ID: &str = "dev.margin.native";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.ends_with("-apple-ios-sim") {
        return;
    }
    let path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("simulator.xcent");
    std::fs::write(
        &path,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>application-identifier</key><string>FAKETEAMID.{BUNDLE_ID}</string></dict></plist>
"#
        ),
    )
    .unwrap();
    println!(
        "cargo:rustc-link-arg-bin=margin=-Wl,-sectcreate,__TEXT,__entitlements,{}",
        path.display()
    );
}
