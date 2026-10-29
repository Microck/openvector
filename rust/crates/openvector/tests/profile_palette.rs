mod common;

use openvector::profile_palette::{apply_palette_command, read_palette_runtime, PaletteCommand, PaletteProfile, PaletteRuntime};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
struct Selection {
    count: f64,
    palette_count: f64,
    palette: Vec<f32>,
    runtime: PaletteRuntime,
}

impl Selection {
    fn apply(&mut self, command: &PaletteCommand) -> Result<Option<bool>, String> {
        apply_palette_command(&mut PaletteProfile {
            color_count: &mut self.count,
            palette_color_count: &mut self.palette_count,
            palette: &mut self.palette,
        }, &mut self.runtime, command)
    }
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn native_palette(value: &Value) -> Vec<f32> {
    hex(value.as_str().unwrap()).chunks_exact(16).flat_map(|pixel| {
        [12, 8, 4, 0].map(|offset| f32::from_le_bytes(pixel[offset..offset + 4].try_into().unwrap()))
    }).collect()
}

fn native_count(settings: &Value, offset: usize) -> f64 {
    let bytes = hex(settings.as_str().unwrap());
    i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as f64
}

fn snapshot(frame: &Value) -> Selection {
    Selection {
        count: native_count(&frame["selected"]["settingsBits"], 8),
        palette_count: native_count(&frame["selected"]["settingsBits"], 56),
        palette: native_palette(&frame["selected"]["paletteArgbBits"]),
        runtime: PaletteRuntime {
            backup: native_palette(&frame["paletteRuntime"]["backupArgbBits"]),
            customized: frame["paletteRuntime"]["customized"].as_bool().unwrap(),
        },
    }
}

#[test]
fn native_profile_recordings_preserve_leaf_command_state_and_revert_ownership() {
    let (mut cases, mut commands, mut snapshots) = (0, 0, 0);
    for frame in common::read_ndjson("profile-palettes-reference.ndjson.gz") {
        if frame.get("id").is_none() { continue; }
        let actions = frame["actions"].as_array().unwrap();
        let expected = frame["expected"].as_array().unwrap();
        assert_eq!(actions.len(), expected.len());
        let mut selected: Option<Selection> = None;
        for (action, reference) in actions.iter().zip(expected) {
            let label = format!("{} {action}", frame["id"]);
            let command = match action[0].as_str().unwrap() {
                "set" => match action[2].as_str().unwrap() {
                    "addColor()" => Some(PaletteCommand::Add),
                    "deleteColor(int)" => Some(PaletteCommand::Delete(action[3].as_f64().unwrap())),
                    "revertPaletteEdits()" => Some(PaletteCommand::Revert),
                    "setAdvNumColors(int)" => Some(PaletteCommand::Count(action[3].as_f64().unwrap())),
                    _ => { selected = None; None }
                },
                "snapshot" | "save" | "document" => None,
                _ => { selected = None; None }
            };
            if let (Some(state), Some(command)) = (&mut selected, command) {
                state.apply(&command).unwrap();
                let bits = &reference["processing"]["controls"]["selectedSettingsBits"];
                assert_eq!(state.count, native_count(bits, 8), "{label} count");
                assert_eq!(state.palette_count, native_count(bits, 56), "{label} palette count");
                commands += 1;
            }
            // Source loading, saved-profile selection and palette discovery belong
            // to the document layer. Their captured snapshots seed each independent
            // leaf-command sequence rather than simulating those external actions.
            if reference.get("selected").is_some() {
                let recorded = snapshot(reference);
                if let Some(state) = &selected {
                    assert_eq!(state, &recorded, "{label}");
                    snapshots += 1;
                }
                selected = Some(recorded);
            }
        }
        cases += 1;
    }
    assert_eq!(cases, 8);
    assert!(commands >= 30, "only {commands} leaf commands replayed");
    assert!(snapshots >= 8, "only {snapshots} native snapshots compared");
}

fn initial_selection() -> Selection {
    Selection {
        count: 2.0,
        palette_count: 2.0,
        palette: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        runtime: PaletteRuntime { backup: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0], customized: false },
    }
}

#[test]
fn commands_keep_native_count_limits_stale_delete_metadata_and_separate_backup() {
    let mut state = initial_selection();
    assert_eq!(state.apply(&PaletteCommand::Count(2.0)).unwrap(), Some(false));
    assert_eq!(state.apply(&PaletteCommand::Count(24.0)).unwrap(), Some(true));
    let backup = state.runtime.backup.clone();
    state.apply(&PaletteCommand::Color { index: 0.0, color: vec![0.25, 0.5, 0.75, 1.0] }).unwrap();
    assert_ne!(state.palette, backup);
    assert_eq!(state.runtime.backup, backup);
    assert!(state.runtime.customized);
    state.apply(&PaletteCommand::Delete(0.0)).unwrap();
    assert_eq!((state.palette.len(), state.palette_count), (4, 2.0));
    state.apply(&PaletteCommand::Revert).unwrap();
    assert_eq!(state.palette, backup);
    assert!(!state.runtime.customized);
    state.apply(&PaletteCommand::Source).unwrap();
    assert!(state.palette.is_empty());
    assert_eq!((state.count, state.palette_count), (24.0, 2.0));
    assert_eq!(state.runtime.backup, backup);
    state.apply(&PaletteCommand::Revert).unwrap();
    for _ in 2..25 { state.apply(&PaletteCommand::Add).unwrap(); }
    assert_eq!((state.palette.len(), state.palette_count), (100, 25.0));
    state.runtime.customized = false;
    let before = state.clone();
    state.apply(&PaletteCommand::Add).unwrap();
    assert_eq!(state, before);
    state.apply(&PaletteCommand::Color { index: -1.0, color: vec![1.0; 4] }).unwrap();
    assert_eq!(state.palette, before.palette);
    assert!(state.runtime.customized);
}

#[test]
fn invalid_commands_preserve_every_owned_field() {
    let commands = [
        PaletteCommand::Count(1.0), PaletteCommand::Count(25.0), PaletteCommand::Count(2.5),
        PaletteCommand::Count(f64::NAN), PaletteCommand::Choose(vec![]), PaletteCommand::Choose(vec![0.0; 4]),
        PaletteCommand::Replace(vec![0.0; 3]), PaletteCommand::Replace(vec![f64::NAN; 4]),
        PaletteCommand::Delete(-1.0), PaletteCommand::Delete(2.0), PaletteCommand::Delete(0.5),
        PaletteCommand::Color { index: 0.0, color: vec![0.0; 8] },
        PaletteCommand::Color { index: 0.5, color: vec![0.0; 4] },
    ];
    let mut state = initial_selection();
    for command in commands {
        let before = state.clone();
        assert!(state.apply(&command).is_err(), "{command:?}");
        assert_eq!(state, before, "{command:?}");
    }
    for command in [Value::Null, json!([]), json!({}), json!({"action":"unknown"})] {
        assert!(PaletteCommand::from_json(&command).is_err());
    }
    for runtime in [Value::Null, json!([]), json!({"backup":[],"customized":0}), json!({"backup":[0],"customized":false})] {
        assert!(read_palette_runtime(&runtime).is_err());
    }
    assert_eq!(read_palette_runtime(&json!({"backup":[],"customized":true})).unwrap(),
        PaletteRuntime { backup: vec![], customized: true });
}

#[test]
fn choose_and_replace_quantize_qcolor_without_aliasing_runtime_backup() {
    let mut state = initial_selection();
    let palette = vec![0.25, 0.5, 0.75, 1.0, 1.0, 0.0, 0.0, 0.0];
    let chosen = PaletteCommand::from_json(&json!({"action":"choose","palette":palette})).unwrap();
    state.apply(&chosen).unwrap();
    let quantized = vec![
        (16384.0 / 65535.0) as f32, (32768.0 / 65535.0) as f32,
        (49151.0 / 65535.0) as f32, 1.0, 1.0, 0.0, 0.0, 0.0,
    ];
    assert_eq!(state.palette, quantized);
    assert_eq!(state.runtime.backup, quantized);
    assert_eq!((state.count, state.palette_count, state.runtime.customized), (2.0, 2.0, false));
    state.palette[0] = 0.0;
    assert_eq!(state.runtime.backup, quantized);
    state.apply(&PaletteCommand::Revert).unwrap();
    assert_eq!(state.palette, quantized);
    state.apply(&PaletteCommand::Replace(vec![])).unwrap();
    assert_eq!((state.count, state.palette_count), (2.0, 0.0));
    assert!(state.palette.is_empty());
    assert!(state.runtime.backup.is_empty());
    assert!(!state.runtime.customized);
}
