// All files live in the worker's private Emscripten filesystem.
use super::{open, render};
use openvector::document_controls::{document_segmentation_key, read_document_settings};
use openvector::{
    export_options::{ExportOptions, read_export_options},
    image_document::ImageDocument,
    segmentation_editor::EditorSnapshot,
};
use serde_json::{Value, json};
use std::{cell::RefCell, fs};

struct Editor {
    document: ImageDocument,
    settings: Value,
    options: ExportOptions,
}

thread_local! { static ACTIVE: RefCell<Option<Editor>> = const { RefCell::new(None) }; }

fn write_pixels(mut bgra: Vec<u8>) -> Result<(), String> {
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    fs::write("pixels", bgra).map_err(|e| e.to_string())
}

fn edit_preview(snapshot: EditorSnapshot) -> Result<Value, String> {
    write_pixels(snapshot.bgra)?;
    Ok(
        json!({"pixels": true, "width": snapshot.width, "height": snapshot.height,
        "tool": snapshot.tool, "color": snapshot.color, "undo": !snapshot.undo.is_empty(),
        "redo": !snapshot.redo.is_empty()}),
    )
}

impl Editor {
    fn command(&mut self, command: &Value) -> Result<Value, String> {
        let doc = &mut self.document;
        let mut reply = json!({});
        match command["action"].as_str() {
            Some("open") => reply["settings"] = self.settings.clone(),
            Some("configure") => {
                let settings = &command["settings"];
                let key = |value: &Value| {
                    read_document_settings(value)
                        .map(|s| document_segmentation_key(&s))
                        .map_err(|e| e.to_string())
                };
                if key(settings)? != key(&self.settings)? {
                    doc.run_task("segmentation", settings, None)?;
                }
                for stage in ["smoothing", "fitting"] {
                    doc.run_task(stage, settings, None)?;
                }
                self.settings = settings.clone();
            }
            Some("analyze") => {
                let count = command["count"].as_u64().ok_or("Choose a color count.")? as usize;
                let palette = doc.preview_palette(&command["settings"], count)?;
                return Ok(json!({"palette":palette}));
            }
            Some("begin") => return edit_preview(doc.begin_editing_for_output()?),
            Some("stroke") => {
                let events = command["events"]
                    .as_array()
                    .filter(|events| !events.is_empty())
                    .ok_or("Empty stroke.")?;
                return edit_preview(doc.grouped_gestures(events)?);
            }
            Some("undo") => return edit_preview(doc.grouped_undo_edit()?),
            Some("redo") => return edit_preview(doc.grouped_redo_edit()?),
            Some("revert") => return edit_preview(doc.revert_edits()?),
            Some("finish") => {
                doc.finish_editing(&self.settings, None)?;
            }
            Some("background") => {
                doc.begin_background_editing()?;
            }
            Some("toggle") => {
                doc.toggle_background(openvector::editor_input::read_editor_point(
                    &command["point"],
                )?)?;
            }
            Some("undo-background") => {
                doc.undo_background();
            }
            Some("redo-background") => {
                doc.redo_background();
            }
            Some("revert-background") => {
                doc.revert_background();
            }
            Some("finish-background") => {
                doc.finish_background_editing();
            }
            Some("export") => {
                self.options = read_export_options(&command["options"], self.options)
                    .map_err(|e| e.to_string())?;
            }
            _ => return Err("Unknown editor action.".into()),
        }
        let background = doc.background_state();
        reply["undo"] = json!(!background.undo.is_empty());
        reply["redo"] = json!(!background.redo.is_empty());
        let svg = render(doc, self.options)?;
        fs::write("output.svg", svg).map_err(|e| e.to_string())?;
        reply["svg"] = json!(true);
        Ok(reply)
    }
}

fn dispatch() -> Result<(), String> {
    let command: Value =
        serde_json::from_slice(&fs::read("command.json").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    ACTIVE.with_borrow_mut(|slot| {
        if command["action"] == "open" {
            let (document, settings, image) = open(
                &fs::read("input").map_err(|e| e.to_string())?,
                command["filename"].as_str().ok_or("Missing filename.")?,
                command["shrink"].as_bool().unwrap_or(false),
            )?;
            let mut candidate = Editor {
                document,
                settings,
                options: ExportOptions::default(),
            };
            let mut reply = candidate.command(&command)?;
            reply["width"] = json!(image.width);
            reply["height"] = json!(image.height);
            reply["pixels"] = json!(true);
            write_pixels(image.bgra)?;
            fs::write("response.json", reply.to_string()).map_err(|e| e.to_string())?;
            *slot = Some(candidate);
            Ok(())
        } else {
            let reply = slot
                .as_mut()
                .ok_or("Open an image first.")?
                .command(&command)?;
            fs::write("response.json", reply.to_string()).map_err(|e| e.to_string())
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn editor_command() -> i32 {
    match dispatch() {
        Ok(()) => 0,
        Err(error) => {
            let _ = fs::write("error.txt", error);
            -1
        }
    }
}
