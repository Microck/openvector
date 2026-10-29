mod common;
use openvector::batch_queue::{BatchQueue, OverwriteDecision};
use serde_json::Value;

#[test]
fn native_queue_actions_and_output_planning() {
    let references = common::read_ndjson("batch-queue-reference.ndjson.gz");
    assert!(!references.is_empty());
    for reference in references {
        let id = reference["id"].as_str().unwrap();
        if let Some(actions) = reference["actions"].as_array() {
            let mut queue = BatchQueue::new(reference["applicationDirectory"].as_str().unwrap(), reference["workingDirectory"].as_str().unwrap());
            for action in actions {
                let input = &action["input"];
                let expected = &action["expected"];
                let mut duplicates = Vec::new();
                match input["action"].as_u64().unwrap() {
                    0 | 8 => {},
                    1 => duplicates = queue.add_files(&input["value"].as_str().unwrap().split('\0').map(str::to_owned).collect::<Vec<_>>()),
                    2 => queue.clear(),
                    3 => queue.remove(input["value"].as_u64().unwrap() as usize).unwrap(),
                    4 => queue.export_directory = input["value"].as_str().unwrap().into(),
                    5 => queue.export_format = input["value"].as_str().unwrap().into(),
                    6 => queue.processing_index = input["value"].as_i64().unwrap() as i32,
                    7 => queue.set_accepting_edits(input["value"].as_i64().unwrap() != 0),
                    9 => queue.select(input["index"].as_u64().unwrap() as usize, input["selected"].as_bool().unwrap()).unwrap(),
                    10 => queue.remove_selected(),
                    other => panic!("unrecognized native action {other}"),
                }
                let actual = queue.snapshot();
                assert_eq!(actual.export_directory, expected["exportDirectory"].as_str().unwrap(), "{id}");
                assert_eq!(actual.export_format, expected["exportFormat"].as_str().unwrap(), "{id}");
                assert_eq!(actual.processing_index as u32 as u64, expected["processingIndex"].as_u64().unwrap(), "{id}");
                assert_eq!(!duplicates.is_empty(), expected["dialogSeen"].as_bool().unwrap(), "{id}");
                let actual_records = actual.queue.iter().map(|r| serde_json::json!({"filename":r.filename,"buttonIndex":r.button_index})).collect::<Vec<_>>();
                let expected_records = expected["queue"].as_array().unwrap().iter().map(|r| serde_json::json!({"filename":r["filename"],"buttonIndex":r["buttonIndex"]})).collect::<Vec<_>>();
                assert_eq!(actual_records, expected_records, "{id}");
            }
        } else if let Some(filenames) = reference["filenames"].as_array() {
            let mut queue = BatchQueue::new(".", "D:/migracion/vector-magic-re");
            queue.add_files(&filenames.iter().map(|f| f.as_str().unwrap().to_owned()).collect::<Vec<_>>());
            queue.export_directory = reference["exportDirectory"].as_str().unwrap().into();
            queue.export_format = reference["exportFormat"].as_str().unwrap().into();
            let existing = reference["existingOutputs"].as_array().unwrap();
            let plan = queue.prepare(|name| existing.iter().any(|v| v.as_str() == Some(name)));
            assert_eq!(serde_json::to_value(&plan.overwrites).unwrap(), reference["existingOutputs"], "{id}");
            assert_eq!(plan.processable, filenames.len() - existing.len(), "{id}");
            let decision = queue.decide_overwrite(OverwriteDecision::try_from(reference["decision"].as_str().unwrap()).unwrap());
            let actual = queue.snapshot();
            let expected = &reference["expected"];
            let outputs = expected["processing"]["outputs"].as_array().unwrap();
            let records = expected["queue"].as_array().unwrap();
            assert_eq!(actual.queue.len(), records.len(), "{id}");
            for ((record, output), native) in actual.queue.iter().zip(outputs).zip(records) {
                // Historical native planning reused a case alias. The approved contract reserves it.
                let expected_output = if id == "planning-case-alias-outputs" {
                    output[0].as_str().unwrap().replace("/A.svg", "/A_1.svg")
                } else { output[0].as_str().unwrap().to_owned() };
                assert_eq!(record.output_filename, expected_output, "{id}");
                assert_eq!(record.should_process, native["initializedFlags"][0].as_i64().unwrap() != 0, "{id}");
            }
            let processable = records.iter().filter(|r| r["initializedFlags"][0] != Value::from(0)).count();
            assert_eq!(decision.processable, processable, "{id}");
            assert_eq!(decision.should_start, reference["decision"] != "cancel" && processable > 0, "{id}");
        } else { panic!("unhandled fixture {id}"); }
    }
}

#[test]
fn selection_snapshot_and_cancellation_preserve_queue() {
    let mut queue = BatchQueue::new("D:/app", "D:/images");
    queue.add_files(&["a.png".into(), "b.png".into()]);
    queue.select(0, true).unwrap();
    queue.set_accepting_edits(false);
    queue.select(1, true).unwrap();
    assert!(queue.snapshot().queue.iter().all(|r| !r.selected));
    assert!(queue.select(2, true).is_err());
    queue.prepare(|name| name.ends_with("a.ai"));
    let before = queue.snapshot();
    let cancelled = queue.decide_overwrite(OverwriteDecision::Cancel);
    assert!(!cancelled.should_start);
    assert_eq!(cancelled.processable, 1);
    assert_eq!(before, queue.snapshot());
    let mut detached = queue.snapshot();
    detached.queue.clear();
    assert_eq!(queue.snapshot().queue.len(), 2);
}
