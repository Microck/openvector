use openvector::{image_document::ImageDocument, palette::CoreSession};
use serde_json::{json, Value};

#[test]
fn regenerated_palette_is_marked_by_the_gui_selection_path() {
    let mut pixels = vec![0u8; 48 * 48 * 4];
    for y in 8..40 { for x in 8..40 {
        let offset = (y * 48 + x) * 4;
        pixels[offset + 2] = 255; pixels[offset + 3] = 128;
    }}
    let mut doc = ImageDocument::new(CoreSession::default());
    doc.set_source(48, 48, &pixels, true, 0.0).unwrap();
    let original = doc.run_automatic(&json!({"extension":".png"})).unwrap();
    let mut controls = Value::Object(original.settings.to_json());
    controls["lower"] = json!(3); controls["upper"] = json!(3);
    doc.run_task("palette", &controls, None).unwrap();
    let palette: Vec<f32> = doc.state().palettes[3].chunks_exact(4)
        .flat_map(|color| color.iter().rev().map(|v| (((*v as f64 * 65535.0 + 0.5).floor()) / 65535.0) as f32))
        .collect();
    assert_ne!(palette, original.settings.palette);
    controls["palette"] = json!(palette);
    controls["paletteMode"] = json!(1);
    doc.remember_generated_palette(1, &palette);
    for stage in ["segmentation", "smoothing", "fitting"] {
        doc.run_task(stage, &controls, None).unwrap();
    }
    let geometry = doc.presentation_geometry().unwrap();
    assert!(geometry.regions.iter().any(|region| region.bgra[2] >= 250 && (120..=135).contains(&region.bgra[3])),
        "generated translucent red should be straight in the output");
}


#[test]
fn grouped_gui_editing_keeps_straight_automatic_colors() {
    let mut pixels = vec![0u8; 48 * 48 * 4];
    for y in 8..40 { for x in 8..40 {
        let offset = (y * 48 + x) * 4;
        pixels[offset + 2] = 255; pixels[offset + 3] = 128;
    }}
    let mut doc = ImageDocument::new(CoreSession::default());
    doc.set_source(48, 48, &pixels, true, 0.0).unwrap();
    let result = doc.run_automatic(&json!({"extension":".png"})).unwrap();
    let initial = doc.begin_editing_for_output().unwrap();
    let offset = (20 * initial.width + 20) * 4;
    assert_eq!(&initial.bgra[offset..offset+4], &[0,0,255,128]);
    doc.grouped_gesture(&json!({"phase":"select","tool":"pencil"})).unwrap();
    doc.grouped_gesture(&json!({"phase":"color","color":[0.0,0.25,0.8,0.5]})).unwrap();
    doc.grouped_gesture(&json!({"phase":"press","point":[10,10],"buttons":1})).unwrap();
    doc.grouped_gesture(&json!({"phase":"move","point":[14,10],"buttons":1})).unwrap();
    doc.grouped_gesture(&json!({"phase":"release","point":[14,10],"buttons":0})).unwrap();
    let edited = doc.editing_state().unwrap().bgra;
    assert_ne!(edited, initial.bgra);
    assert_eq!(doc.grouped_undo_edit().unwrap().bgra, initial.bgra);
    assert_eq!(doc.grouped_redo_edit().unwrap().bgra, edited);
    doc.finish_editing(&Value::Object(result.settings.to_json()), None).unwrap();
    let geometry = doc.presentation_geometry().unwrap();
    assert!(geometry.regions.iter().any(|r| r.bgra[2] >= 250 && (120..=135).contains(&r.bgra[3])));
}
