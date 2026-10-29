mod common;
mod editor_support;

use editor_support::{compare_json, compare_settings, hex};
use openvector::{image_document::CANCEL_TASK, palette::CoreSession};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

#[test]
fn native_direct_integer_observer_and_caller_unwind_cancellations() {
    let (mut cases, mut completed, mut cancelled, mut unwound) = (0, 0, 0, 0);
    for frame in common::read_ndjson("document-cancellation-reference.ndjson.gz") {
        if frame["type"] == "metadata" {
            assert_eq!(frame["wholeNativeResponsesRepeatIdentical"], true);
            continue;
        }
        let session = CoreSession::default();
        let pixels = common::b64(frame["bgra"].as_str().unwrap());
        let before = pixels.clone();
        let mut document = session.create_document();
        document.set_source(frame["width"].as_u64().unwrap() as usize, frame["height"].as_u64().unwrap() as usize, &pixels, true, 0.0).unwrap();
        let mut previous_percent = 0;
        let steps = frame["steps"].as_array().unwrap();
        assert_eq!(steps.len(), frame["states"].as_array().unwrap().len());
        for (index, step) in steps.iter().enumerate() {
            let reference = &frame["states"][index];
            let label = format!("{} step {index}", frame["id"]);
            let v = step["values"].as_array().unwrap();
            let input = json!({"lower":v[0],"upper":v[1],"category":v[2],"quality":v[3],"sensitivity":v[4],"advanced":v[5].as_i64().unwrap()!=0,"antiAlias":v[6].as_i64().unwrap()!=0,"backgroundMode":v[7],"background":step["background"],"complexity":5,"minimumPixels":0,"artifactRejection":0,"clusterColors":true,"paletteMode":2,"palette":[],"smoothness":6,"detectCorners":true,"curveComplexity":6,"smoothingAntiAlias":true});
            let cancel_at = step["cancelAt"].as_i64().unwrap();
            if cancel_at < 0 { document.request_cancel(); }
            let cancel = document.cancellation_handle();
            let mut events: Vec<Value> = Vec::new();
            let mut requested = false;
            let result = document.analyze(&input, Some(&mut |event, _| {
                // The Qt observer keeps its previous percentage across calls;
                // direct and caller-unwind recordings observe every checkpoint.
                if frame["operation"] == 98 && event.percent == previous_percent { return Ok(()); }
                previous_percent = event.percent;
                let cancelled_before = requested;
                if cancel_at > 0 && events.len() as i64 + 1 == cancel_at {
                    cancel.store(true, Ordering::Relaxed);
                    requested = true;
                }
                events.push(json!({"ordinal":events.len()+1,"stage":1,"colorCount":event.color_count.unwrap(),"phase":event.iteration.unwrap()*5,"randomState":session.random_state(),"cancelledBefore":cancelled_before,"cancelledAfter":requested,"fractionBits":event.fraction.to_le_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>()}));
                Ok(())
            }));
            if reference["callReturned"].as_bool().unwrap() {
                result.unwrap_or_else(|error| panic!("{label}: {error}")); completed += 1;
            } else {
                assert_eq!(result.unwrap_err(), CANCEL_TASK, "{label}"); cancelled += 1;
            }
            // The recorder emits this field only when the native caller unwinds.
            if reference.get("callerCaught").is_some() {
                assert_eq!(reference["callerCaught"], true, "{label}");
                assert_eq!(frame["operation"], 99, "{label}");
                assert_eq!(reference["prePostUnwindStateIdentical"], true, "{label}"); unwound += 1;
            }
            compare_json(&json!(events), &reference["events"], &label);
            let state = document.snapshot(); let native = &reference["state"];
            // The recorder's first-chance status is a no-return sentinel, not
            // a status returned by the operation that threw cancellation.
            if reference["callReturned"].as_bool().unwrap() {
                assert_eq!(state.status as u64, native["status"].as_u64().unwrap(), "{label}");
            }
            compare_json(&json!([state.progress,state.cancelled,state.processed,state.random_state]), &json!([native["progress"],native["cancelled"],native["processed"],native["randomState"]]), &label);
            assert_eq!(state.stage_keys.palette, native["cacheKey"].as_str().unwrap(), "{label}");
            compare_json(&json!(state.completed), &native["completed"], &label);
            assert_eq!(state.scores.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(), hex(native["scoreBits"].as_str().unwrap()), "{label} score bits");
            assert_eq!(state.palettes.len(), 25);
            assert_eq!(native["paletteBits"].as_array().unwrap().len(), 25);
            for (count, palette) in state.palettes.iter().enumerate() {
                assert_eq!(palette.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(), hex(native["paletteBits"][count].as_str().unwrap()), "{label} palette {count}");
            }
            for (kind, raster) in [("source", &state.source), ("working", &state.working)] {
                compare_json(&json!([raster.width,raster.height]), &native[format!("{kind}Dimensions")], &label);
                assert_eq!(raster.bgra, common::b64(native[format!("{kind}Bgra")].as_str().unwrap()), "{label} {kind}");
            }
            compare_settings(&state.settings, native["settingsBits"].as_str().unwrap(), "");
            assert!(state.settings.palette.is_empty(), "{label}");
            assert_eq!(state.original.bgra, common::b64(frame["originalBgra"].as_str().unwrap()), "{label} original");
            assert_eq!(pixels, before, "{label} caller input");
        }
        cases += 1;
    }
    assert_eq!((cases,cancelled,unwound), (234,144,48));
    assert!(completed > 216);
}
