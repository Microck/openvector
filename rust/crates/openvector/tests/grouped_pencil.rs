use openvector::{editor_input::EditorPointer, segmentation_editor::SegmentationEditor};
use serde_json::json;

fn editor() -> SegmentationEditor {
    let pixels = vec![0u8; 8 * 8 * 4];
    SegmentationEditor::new(8, 8, &pixels, &pixels, &[0.0, 0.0, 0.0, 0.0], &[0.0, 0.0, 1.0, 1.0], 0).unwrap()
}
fn event(e: &mut SegmentationEditor, p: &mut EditorPointer, phase: &str, x: f64, y: f64, buttons: u32) {
    e.grouped_gesture(&json!({"phase":phase,"point":[x,y],"buttons":buttons}), p).unwrap();
}
#[test]
fn grouped_strokes_undo_and_redo_as_units() {
    let mut e = editor();
    let mut p = EditorPointer::default();
    e.grouped_gesture(&json!({"phase":"select","tool":"pencil"}), &mut p).unwrap();
    let empty=e.snapshot().bgra;
    event(&mut e,&mut p,"press",1.0,1.0,1);
    let returned=e.grouped_gesture(&json!({"phase":"move","point":[3,1],"buttons":1}), &mut p).unwrap();
    assert_eq!(returned, e.snapshot());
    event(&mut e,&mut p,"move",5.0,1.0,1);
    event(&mut e,&mut p,"release",5.0,1.0,0);
    let first=e.snapshot().bgra;
    assert_ne!(first,empty);
    event(&mut e,&mut p,"press",1.0,3.0,1);
    event(&mut e,&mut p,"move",5.0,3.0,1);
    event(&mut e,&mut p,"release",5.0,3.0,0);
    let second=e.snapshot().bgra;
    assert_ne!(second,first);
    assert_eq!(e.grouped_undo().unwrap().bgra, first);
    assert_eq!(e.grouped_undo().unwrap().bgra, empty);
    assert_eq!(e.grouped_redo().unwrap().bgra, first);
    assert_eq!(e.grouped_redo().unwrap().bgra, second);
    let checkpoint=e.history_checkpoint();
    e.grouped_undo().unwrap();
    e.restore_checkpoint(&checkpoint).unwrap();
    assert_eq!(e.grouped_undo().unwrap().bgra, first);
}
#[test]
fn legacy_gesture_undo_remains_per_event() {
    let mut e=editor(); let mut p=EditorPointer::default();
    e.gesture(&json!({"phase":"select","tool":"pencil"}),&mut p).unwrap();
    e.gesture(&json!({"phase":"press","point":[1,1],"buttons":1}),&mut p).unwrap();
    let first=e.snapshot().bgra;
    e.gesture(&json!({"phase":"move","point":[4,1],"buttons":1}),&mut p).unwrap();
    assert_eq!(e.undo().unwrap().bgra,first);
}
