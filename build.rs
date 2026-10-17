use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml");
    let manifest: toml::Table = fs::read_to_string(&path).unwrap().parse().unwrap();
    let lib_name = manifest
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| std::env::var("CARGO_PKG_NAME").unwrap().replace('-', "_"));

    const LAYER_NAME: &str = "VK_LAYER_COMPAT";

    // CARGO_CFG_TARGET_OS is the *target* OS, so cross-compiling gets the right name.
    let library_path = match std::env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
        "windows" => format!(".\\\\{lib_name}.dll"),
        "macos" | "ios" => format!("./lib{lib_name}.dylib"),
        _ => format!("./lib{lib_name}.so"),
    };

    let manifest = format!(
        r#"{{
    "file_format_version": "1.2.0",
    "layer": {{
        "name": "{LAYER_NAME}",
        "type": "INSTANCE",
        "library_path": "{library_path}",
        "api_version": "1.4.352",
        "implementation_version": "1",
        "description": "Vulkan layer to make games work with ugly hacks"
    }}
}}
"#
    );

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile_dir = out_dir.ancestors().nth(4).expect("unexpected OUT_DIR layout");
    fs::write(profile_dir.join(format!("{lib_name}.json")), manifest).unwrap();
}
