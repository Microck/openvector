mod support;
#[path="../../openvector/tests/common/mod.rs"] mod common;
use std::{process::Command, time::{Duration, Instant}};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use support::FixtureDirectory;

#[test]
#[ignore = "Requires a real or Xvfb display and Vulkan ICD"]
fn native_window_opens_processes_and_exports() {
    let directory = FixtureDirectory::new();
    let reference = common::read_ndjson("fresh-conversion-20261020-exports.ndjson.gz").into_iter()
        .find(|row| row["id"] == "fresh-conversion-20261020-00-logo-5x7-svg-batch").expect("retained native SVG capture");
    assert_eq!(reference["exportBytesRepeatIdentical"], true);
    assert_eq!(reference["exports"][0], reference["exports"][1], "independent native runs agree byte-for-byte");
    let input = directory.0.join(reference["input"]["name"].as_str().unwrap());
    let output = directory.0.join("result.svg");
    std::fs::write(&input, STANDARD.decode(reference["input"]["bytes"].as_str().unwrap()).unwrap()).unwrap();
    let expected = STANDARD.decode(reference["exports"][0].as_object().unwrap().values().next().unwrap()["bytes"].as_str().unwrap()).unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_openvector-gui"))
        .arg("--smoke").arg(&input).arg(&output)
        .env("OPENVECTOR_PREFERENCES", directory.0.join("preferences.json"))
        .spawn().expect("launch the native GPUI executable");
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = process.try_wait().unwrap() { break status; }
        if Instant::now() >= deadline {
            process.kill().expect("stop hung GUI smoke process");
            process.wait().unwrap();
            panic!("GUI failed to settle its load/vectorize/export workflow within 180 seconds");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "GUI exited with {status}");
    let svg = std::fs::read(output).expect("GUI exports its real fitted output");
    assert_eq!(svg, expected, "headed GUI output differs from the retained Windows-native SVG bytes");
}
