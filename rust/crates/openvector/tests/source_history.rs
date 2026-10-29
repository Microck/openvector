mod common;

use openvector::bmp_input::decode_bmp;
use openvector::source_history::{SourceDirectories, SourceHistory, format_file_size};
use serde_json::json;

#[test]
fn recent_file_rows_match_native_captures() {
    for reference in common::read_ndjson("source-history-reference.ndjson.gz").into_iter().skip(1) {
        let mut history = SourceHistory::default();
        for (index, action) in reference["actions"].as_array().unwrap().iter().enumerate() {
            let expected = &reference["expected"][index];
            match action[0].as_str().unwrap() {
                "open" | "open-wide" => {
                    let filename = action[1].as_str().unwrap();
                    let bytes = common::b64(reference["inputs"][filename].as_str().unwrap());
                    let image = decode_bmp(&bytes).unwrap();
                    history.add(filename, image.width as u32, image.height as u32, bytes.len() as u64).unwrap();
                }
                "remove-file" => { history.remove(action[1].as_str().unwrap()); }
                "set" => {
                    assert_eq!(action[2], "clearMruFiles()");
                    history.clear();
                }
                "describe-size" => assert_eq!(
                    format_file_size(action[1].as_u64().unwrap()).unwrap(),
                    expected["sizeDescription"]["text"].as_str().unwrap(),
                    "{} action {index}", reference["id"],
                ),
                "snapshot" | "document" => {},
                kind => panic!("unhandled native action {kind}"),
            }
            if let Some(native) = expected.get("sourceHistory") {
                assert_eq!(history.snapshot(), json!({"capacity":native["capacity"],"files":native["files"]}),
                    "{} action {index}", reference["id"]);
            }
        }
    }
}

#[test]
fn recent_directories_match_native_across_restart() {
    for reference in common::read_ndjson("source-directories-reference.ndjson.gz") {
        let mut history = SourceDirectories::default();
        for (phase, actions) in reference["phases"].as_array().unwrap().iter().enumerate() {
            if phase > 0 { history = SourceDirectories::from_snapshot(&history.snapshot()).unwrap(); }
            for (index, action) in actions.as_array().unwrap().iter().enumerate() {
                match action[0].as_str().unwrap() {
                    "open" | "open-wide" => {
                        // These captures contain absolute Windows filenames. The host owns dirname.
                        let filename = action[1].as_str().unwrap().replace('\\', "/");
                        history.add(filename.rsplit_once('/').unwrap().0).unwrap();
                    }
                    "set" => assert_eq!(action[2], "clearMruFiles()"),
                    "snapshot" | "persist" => {},
                    kind => panic!("unhandled directory action {kind}"),
                }
                if let Some(native) = reference["expected"][phase][index].get("sourceDirectories") {
                    assert_eq!(&history.snapshot(), native, "{} phase {phase} action {index}", reference["id"]);
                } else { assert_eq!(action[0], "set"); }
            }
        }
    }
}

#[test]
fn invalid_history_changes_preserve_prior_rows() {
    let mut history = SourceHistory::default();
    history.add("image.bmp", 13, 7, 334).unwrap();
    let expected = history.snapshot();
    for (filename, width, height, bytes) in [
        ("",17,11,626), ("other.bmp",0,11,626),
        ("other.bmp",17,0,626), ("other.bmp",17,11,9_007_199_254_740_992),
    ] {
        assert!(history.add(filename,width,height,bytes).is_err());
        assert_eq!(history.snapshot(), expected);
    }
    assert_eq!(SourceHistory::thumbnail_dimensions(1,33).unwrap(), [0,0]);
    assert_eq!(SourceHistory::from_snapshot(&expected).unwrap().snapshot(), expected);
    for malformed in [
        json!({"capacity":4,"files":[]}),
        json!({"capacity":3,"files":[{"filename":"a","description":"x","thumbnailDimensions":[0,1]}]}),
        json!({"capacity":3,"files":[{"filename":"a","description":"x","thumbnailDimensions":[31,31]}]}),
        json!({"capacity":3,"files":[{"filename":"","description":"x","thumbnailDimensions":[32,1]}]}),
    ] { assert!(SourceHistory::from_snapshot(&malformed).is_err()); }
    let mut directories = SourceDirectories::default();
    directories.add("D:/naïve").unwrap();
    directories.add("D:/NAÏVE").unwrap();
    assert_eq!(directories.snapshot(), json!({"capacity":4,"directories":["D:/NAÏVE","D:/naïve"]}));
    let expected = directories.snapshot();
    assert!(directories.add("").is_err());
    assert_eq!(directories.snapshot(), expected);
    for malformed in [
        json!({"capacity":3,"directories":[]}),
        json!({"capacity":4,"directories":["same","same"]}),
        json!({"capacity":4,"directories":[""]}),
        json!({"capacity":4,"directories":[null]}),
        json!({"capacity":4,"directories":["a","b","c","d","e"]}),
    ] { assert!(SourceDirectories::from_snapshot(&malformed).is_err()); }
}
