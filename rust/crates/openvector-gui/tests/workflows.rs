#[path="../../openvector/tests/common/mod.rs"] mod common;
use common::read_ndjson as fixture;
use gpui::Focusable;
use openvector_gui::{OpenVectorApp, viewer::{geometry::{Point, Size, ViewerView}, input::{self, ViewerInput, ViewerKey, KeyPhase, LayerAvailability, PointerPhase, DragAction}, zoom}, wizard::{Page, Wizard}};
use openvector::document_controls::DocumentSettings;
use serde_json::Value;

fn bits(value: &Value) -> u64 {
    let text = value.as_str().unwrap();
    let bytes: Vec<u8> = (0..16).step_by(2).map(|offset| u8::from_str_radix(&text[offset..offset + 2], 16).unwrap()).collect();
    u64::from_le_bytes(bytes.try_into().unwrap())
}

#[gpui::test]
fn global_keyboard_start_and_back_route_view_state(cx: &mut gpui::TestAppContext) {
    let path = std::env::temp_dir().join(format!("openvector-gui-keyboard-{}.json", std::process::id()));
    let (app, cx) = cx.add_window_view(|window, cx| {
        let app = OpenVectorApp::with_preferences(path, cx).unwrap();
        app.focus_handle(cx).focus(window, cx);
        app
    });
    app.update(cx, |app, _| app.wizard.page = Page::Mode);
    cx.simulate_keystrokes("alt-r");
    app.read_with(cx, |app, _| assert_eq!(app.wizard.page, Page::Load));
    app.update(cx, |app, _| app.wizard.page = Page::Mode);
    cx.simulate_keystrokes("pageup");
    app.read_with(cx, |app, _| assert_eq!(app.wizard.page, Page::Load));
}

#[test]
fn native_slider_getter_and_inverse_fixtures_are_exact() {
    let rows = fixture("viewer-zoom-spans-reference.ndjson.gz");
    let mut scales = 0;
    let mut inverses = 0;
    for row in rows {
        match row["kind"].as_str().unwrap() {
            "scale" => {
                scales += 1;
                let view = zoom::slider(&ViewerView::default(), row["position"].as_i64().unwrap());
                let producer = f64::from_bits(bits(&row["producerBitsLE"])).clamp(0.2, 24.0);
                assert_eq!(view.zoom.to_bits(), producer.to_bits(), "slider {}", row["position"]);
                assert_eq!(zoom::export_scale(&view).to_bits(), bits(&row["getterBitsLE"]));
                assert_eq!(zoom::scale_position(f64::from_bits(bits(&row["getterBitsLE"]))), Some(row["inverseGetter"].as_u64().unwrap() as usize));
            }
            "inverse" => {
                inverses += 1;
                assert_eq!(zoom::scale_position(f64::from_bits(bits(&row["bitsLE"]))), Some(row["position"].as_u64().unwrap() as usize));
            }
            "threshold" => {
                let value = bits(&row["bitsLE"]);
                if value != 0 {
                    for (delta, key) in [(-1i64, "below"), (0, "at"), (1, "above")] {
                        assert_eq!(zoom::scale_position(f64::from_bits(value.wrapping_add_signed(delta))), Some(row[key].as_u64().unwrap() as usize));
                    }
                }
            }
            kind => panic!("Unknown zoom fixture kind: {kind}"),
        }
    }
    assert_eq!(scales, 301);
    assert_eq!(inverses, 4099);
}

#[test]
fn viewer_key_sequences_replay_every_native_frame() {
    let rows = fixture("viewer-input-reference.ndjson.gz");
    let mut count = 0;
    for row in rows.iter().filter(|row| row["actions"].is_array()) {
        let mut input = ViewerInput::default(); input.set_layer(2, true, true); input.set_tool(7);
        for (index, action) in row["actions"].as_array().unwrap().iter().enumerate() {
            count += 1;
            let expected = &row["expectedFrames"][index];
            match action["action"].as_i64().unwrap() {
                25 => {
                    let key = action["key"].as_i64().unwrap();
                    let modifiers = action["modifiers"].as_u64().unwrap() ^ if key == input::KEY_SHIFT { input::MODIFIER_SHIFT as u64 } else { 0 };
                    assert_eq!(Some(modifiers), expected["keyModifiers"].as_u64());
                    input.key(ViewerKey { phase: if action["phaseKey"] == "press" { KeyPhase::Press } else { KeyPhase::Release }, key, modifiers }, LayerAvailability { bitmap_enabled: true, vector_enabled: true }).unwrap();
                }
                6 => { input.select_tool(action["tool"].as_i64().unwrap() as i32); }
                10 => { input.set_layer(1, true, true); input.set_tool(0); }
                29 => { input.set_layer(action["value"].as_i64().unwrap() as i32, true, true); }
                28 => { input.set_batch_mode(action["value"].as_i64().unwrap() != 0); }
                26 | 27 | 30 => {}, action => panic!("Unknown fixture action {action}"),
            }
            let state = input.snapshot();
            assert_eq!(state.tool as i64, expected["inputMode"].as_i64().unwrap(), "{} frame {index}", row["id"]);
            if let Some(frame) = expected["viewerInput"].as_array() {
                let actual = [state.primary_layer, state.base_layer, state.tool, i32::from(state.shift_held), state.pan_restore, i32::from(state.held_vector), i32::from(state.held_bitmap), i32::from(state.blocked)];
                assert_eq!(actual.to_vec(), frame[3..].iter().map(|value| value.as_i64().unwrap() as i32).collect::<Vec<_>>(), "{} frame {index}", row["id"]);
            }
            if let Some(frame) = expected["viewerLayers"].as_array() {
                assert_eq!([state.primary_layer, state.base_layer, state.current_layer].to_vec(), frame[3..6].iter().map(|value| value.as_i64().unwrap() as i32).collect::<Vec<_>>());
            }
        }
    }
    assert_eq!(count, 99);
}

#[test]
fn captured_pointer_chords_preserve_native_press_release_policy() {
    let value = common::read_json("browser-pointer-chords.json");
    let expected = [(PointerPhase::Move, 0, 0), (PointerPhase::Press, 2, 2), (PointerPhase::Press, 3, 1), (PointerPhase::Release, 2, 1), (PointerPhase::Release, 0, 2)];
    let actual = value["events"].as_array().unwrap().iter().map(|event| {
        let pointer = input::dom_pointer(event["button"].as_i64().unwrap() as i32, event["buttons"].as_u64().unwrap() as u32, 0);
        (pointer.phase, pointer.buttons, pointer.button)
    }).collect::<Vec<_>>();
    assert_eq!(actual, expected);
    let release = input::dom_pointer(2, 1, 0);
    assert_eq!(input::drag_action(release, false, true), DragAction::EndPan);
}

#[test]
fn fit_wheel_and_one_keep_native_viewer_geometry() {
    let area = Size::new(800.0, 600.0);
    let fitted = zoom::fit(&ViewerView::default(), Size::new(1600.0, 800.0), area).unwrap();
    assert_eq!(fitted.zoom, 0.5);
    let unchanged = zoom::wheel(&fitted, 11, Point::ZERO);
    assert_eq!(unchanged.slider, fitted.slider);
    assert_eq!(zoom::wheel(&fitted, 120, Point::ZERO).slider, fitted.slider + 10);
    assert_eq!(zoom::one(&fitted).zoom.to_bits(), 1.0f64.to_bits());
}

#[test]
fn wizard_back_graph_keeps_alpha_and_review_policy() {
    let mut wizard = Wizard { page: Page::Quality, ..Wizard::default() };
    let mut settings = DocumentSettings::default();
    assert_eq!(wizard.back_page(&settings, false), Page::ImageType);
    assert_eq!(wizard.back_page(&settings, true), Page::Transparency);
    wizard.page = Page::Export;
    assert_eq!(wizard.back_page(&settings, false), Page::Review);
    wizard.skip_review = true;
    assert_eq!(wizard.back_page(&settings, false), Page::Palette);
    wizard.page = Page::Review;
    assert_eq!(wizard.back_page(&settings, false), Page::Export);
    assert_eq!(wizard.completed_page(&settings), Page::Export);
    wizard.skip_review = false;
    assert_eq!(wizard.back_page(&settings, false), Page::Palette);
    assert_eq!(wizard.completed_page(&settings), Page::Review);
    settings.palette_mode = 0;
    assert_eq!(wizard.back_page(&settings, false), Page::Colors);
    wizard.skip_review = true;
    assert_eq!(wizard.back_page(&settings, false), Page::Export);
    wizard.page = Page::Export;
    settings.advanced = true;
    assert_eq!(wizard.back_page(&settings, false), Page::Background);
    assert_eq!(wizard.completed_page(&settings), Page::Export);
}
