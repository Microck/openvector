//! Embeds the Windows app icon and version information into openvector-gui.exe.
//!
//! GPUI's Windows backend loads the window/taskbar icon from resource ID 1 of
//! the running executable (gpui_windows `load_icon`), and Explorer reads the
//! same resource for the file icon. The `.ico` is generated from
//! `assets/app-icon.svg`:
//!
//!     magick -background none -density 384 assets/app-icon.svg \
//!       -define icon:auto-resize=256,64,48,40,32,24,20,16 assets/app-icon.ico
//!     magick -background none -density 96 assets/app-icon.svg -resize 256x256 assets/app-icon-256.png
//!
//! The macOS bundle icon (used by tools/package-gui.mjs) is the Icon Composer
//! document `assets/OpenVector.icon`, compiled on a Mac with Xcode 26 or later:
//!
//!     xcrun actool assets/OpenVector.icon --compile assets/macos --platform macosx \
//!       --minimum-deployment-target 11.0 --app-icon OpenVector --include-all-app-icons \
//!       --output-partial-info-plist /tmp/openvector-icon.plist
//!
//! GNU targets compile the script with `windres` (override with `WINDRES`);
//! MSVC targets use `rc.exe` (override with `RC`). A missing compiler fails the
//! build: an exe without its icon is not a shippable artifact.
use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/app-icon.ico");
    println!("cargo:rerun-if-env-changed=WINDRES");
    println!("cargo:rerun-if-env-changed=RC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let icon = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("assets/app-icon.ico");
    let version = env::var("CARGO_PKG_VERSION").expect("package version");
    let numeric = version.split(['-', '+']).next().unwrap_or("0.0.0").split('.')
        .map(|part| part.parse::<u16>().unwrap_or(0)).chain([0; 4]).take(4)
        .map(|part| part.to_string()).collect::<Vec<_>>().join(",");
    let script = out.join("openvector-gui.rc");
    std::fs::write(&script, format!(r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "OpenVector"
      VALUE "ProductName", "OpenVector"
      VALUE "InternalName", "openvector-gui"
      VALUE "OriginalFilename", "openvector-gui.exe"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
      VALUE "LegalCopyright", "MIT License"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#, icon = icon.display().to_string().replace('\\', "/"))).expect("write resource script");

    let gnu = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");
    let (tool, object, args): (String, PathBuf, Vec<String>) = if gnu {
        let target = env::var("TARGET").expect("TARGET");
        let tool = env::var("WINDRES").unwrap_or_else(|_| format!("{}-w64-mingw32-windres", target.split('-').next().unwrap_or("x86_64")));
        let object = out.join("openvector-gui-resource.o");
        (tool, object.clone(), vec!["--input-format=rc".into(), "--output-format=coff".into(), "-i".into(), script.display().to_string(), "-o".into(), object.display().to_string()])
    } else {
        let object = out.join("openvector-gui.res");
        (env::var("RC").unwrap_or_else(|_| "rc".into()), object.clone(), vec!["/nologo".into(), format!("/fo{}", object.display()), script.display().to_string()])
    };
    let status = Command::new(&tool).args(&args).status()
        .unwrap_or_else(|error| panic!("cannot run Windows resource compiler `{tool}` ({error}); install it or set WINDRES/RC"));
    assert!(status.success(), "`{tool}` failed to compile {}", script.display());
    // Only the GUI binary carries the resource; the library and tests do not.
    println!("cargo:rustc-link-arg-bin=openvector-gui={}", object.display());
}
