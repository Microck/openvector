use std::process::Command;

#[test]
fn vectorize_help_explains_svg_point_and_pixel_dimensions() {
    let output = Command::new(env!("CARGO_BIN_EXE_openvector"))
        .args(["vectorize", "--help"]).output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("72 per inch"));
    assert!(help.contains("48 pt renders as 64"));
    assert!(help.contains("physical print size"));
}
