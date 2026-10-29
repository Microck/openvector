#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

use std::{env, fs, path::PathBuf};

fn main() {
    // OpenVector embeds the two HLSL translation units on every build host.
    // D3DCompile consumes these bytes when the Windows renderer creates its shaders;
    // neither an SDK compiler nor the original source directory ships with the app.
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let common_path = "src/alpha_correction.hlsl";
    println!("cargo:rerun-if-changed={common_path}");
    let common = fs::read_to_string(common_path).expect("read shared HLSL source");
    for name in ["shaders.hlsl", "color_text_raster.hlsl"] {
        let path = format!("src/{name}");
        println!("cargo:rerun-if-changed={path}");
        let source = fs::read_to_string(&path).expect("read HLSL translation unit");
        let body = source
            .strip_prefix("#include \"alpha_correction.hlsl\"")
            .expect("pinned HLSL translation units include alpha_correction.hlsl first");
        fs::write(out.join(name), format!("{common}\n{body}"))
            .expect("write embedded HLSL translation unit");
    }
}
