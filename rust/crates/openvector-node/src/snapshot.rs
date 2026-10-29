//! Node transport only. Bulk buffers are owned copies, never views of live Rust state.
use crate::engine_error;
use napi::{Env, Result, bindgen_prelude::{Buffer, Float32Array, Int32Array, Uint8Array,
    Object, Array, ToNapiValue, JsValue, JsObjectValue, Function, FnArgs}};
use napi_derive::napi;
use openvector::{image_document::DocumentSnapshot, segmentation_editor::EditorSnapshot,
    background_editor::BackgroundSnapshot};
use serde_json::{Value, json};

#[derive(Default)]
#[napi(object)]
pub struct SnapshotSelection {
    pub rasters: Option<Vec<String>>,
    pub segmentation: Option<String>,
    pub contours: Option<bool>,
    pub smoothing: Option<bool>,
    pub fitting: Option<bool>,
}

impl SnapshotSelection {
    pub fn complete() -> Self {
        Self { rasters: Some(vec!["original".into(), "source".into(), "working".into()]),
            segmentation: Some("full".into()), contours: Some(true), smoothing: Some(true), fitting: Some(true) }
    }

    pub fn validate(&self) -> Result<()> {
        if self.rasters.as_ref().is_some_and(|names| names.iter().any(|name|
            !matches!(name.as_str(), "original" | "source" | "working"))) {
            return Err(engine_error("Snapshot rasters must be original, source or working."));
        }
        if self.segmentation.as_ref().is_some_and(|mode| !matches!(mode.as_str(), "regions" | "full")) {
            return Err(engine_error("Snapshot segmentation must be regions or full."));
        }
        Ok(())
    }
}

/// Parse once before processing. Callbacks consume the supplied observer state,
/// never a reentrant document getter while Rust holds the document mutably.
pub struct ProgressOptions<'env> {
    pub callback: Function<'env, FnArgs<(Value, Object<'env>)>, bool>,
    pub capture: SnapshotSelection,
}
impl<'env> ProgressOptions<'env> {
    pub fn read(options: Object<'env>) -> Result<Self> {
        let callback = options.get_named_property("callback")?;
        let capture = options.get_named_property::<Option<SnapshotSelection>>("capture")?.unwrap_or_default();
        capture.validate()?;
        Ok(Self { callback, capture })
    }
}

/// Only bounded settings/parameter/palette metadata uses serde. No raster,
/// region, geometry or variable-length command history enters this value.
pub fn summary(state: &DocumentSnapshot) -> Value {
    json!({
        "dimensions": {"original": [state.original.width, state.original.height],
            "source": [state.source.width, state.source.height], "working": [state.working.width, state.working.height]},
        "settings": state.settings.to_json(),
        "classification": {"category": state.classification.category, "quality": state.classification.quality,
            "paletteMode": state.classification.palette_mode, "colorCount": state.classification.color_count},
        "parameters": state.parameters,
        "stageKeys": {"palette": state.stage_keys.palette, "reservedC0": state.stage_keys.reserved_c0,
            "segmentation": state.stage_keys.segmentation, "segmentationEdits": state.stage_keys.segmentation_edits,
            "smoothing": state.stage_keys.smoothing, "fitting": state.stage_keys.fitting},
        "completed": state.completed, "scores": state.scores, "palettes": state.palettes,
        "available": {"segmentation": state.segmentation.is_some(), "contours": state.contours.is_some(),
            "smoothing": state.smoothing.is_some(), "fitting": state.fitting.is_some()},
        "fittingComplete": state.fitting.as_ref().is_some_and(|fitting| fitting.complete),
        "processed": state.processed, "status": state.status, "progress": state.progress,
        "cancelled": state.cancelled, "randomState": state.random_state,
    })
}

pub fn capture<'env>(env: Env, state: &DocumentSnapshot, selection: &SnapshotSelection) -> Result<Object<'env>> {
    capture_with_summary(env, state, selection, &summary(state))
}

pub fn capture_with_summary<'env>(env: Env, state: &DocumentSnapshot, selection: &SnapshotSelection,
    metadata: &Value) -> Result<Object<'env>>
{
    let mut object = env.to_js_value(metadata)?.coerce_to_object()?;
    if let Some(names) = &selection.rasters {
        let mut captured_rasters = 0u8;
        for name in names {
            let (mask, raster) = match name.as_str() { "original" => (1, &state.original),
                "source" => (2, &state.source), "working" => (4, &state.working),
                _ => unreachable!("validated snapshot selection") };
            if captured_rasters & mask != 0 { continue; }
            captured_rasters |= mask;
            let image = if raster.width == 0 { None } else {
                let mut image = Object::new(&env)?;
                image.set("width", raster.width as u32)?;
                image.set("height", raster.height as u32)?;
                image.set("bgra", Buffer::from(raster.bgra.clone()))?;
                Some(image)
            };
            object.set(name, image)?;
        }
    }
    if let Some(mode) = &selection.segmentation {
        let segmentation = state.segmentation.as_ref().map(|map| {
            let mut object = Object::new(&env)?;
            object.set("width", map.width as u32)?;
            object.set("height", map.height as u32)?;
            if mode == "full" { object.set("indices", map.indices.clone().map(Int32Array::new))?; }
            object.set("palette", Float32Array::new(map.palette.clone()))?;
            object.set("counts", Int32Array::new(map.counts.clone()))?;
            object.set("colorIndices", Int32Array::new(map.color_indices.clone()))?;
            Ok::<_, napi::Error>(object)
        }).transpose()?;
        object.set("segmentation", segmentation)?;
    }
    if selection.contours == Some(true) {
        object.set("contours", state.contours.as_ref().map(|g| contours(env, g)).transpose()?)?;
    }
    if selection.smoothing == Some(true) {
        let smoothing = state.smoothing.as_ref().map(|s| {
            let mut object = Object::new(&env)?;
            object.set("borderCounts", s.border_counts)?;
            object.set("referencePositions", Float32Array::new(s.reference_positions.clone()))?;
            object.set("penaltyCounts", Uint8Array::new(s.penalty_counts.clone()))?;
            Ok::<_, napi::Error>(object)
        }).transpose()?;
        object.set("smoothing", smoothing)?;
    }
    if selection.fitting == Some(true) {
        let fitting = state.fitting.as_ref().map(|f| {
            let mut object = Object::new(&env)?;
            object.set("complete", f.complete)?;
            object.set("geometry", fitted_geometry(env, &f.geometry)?)?;
            object.set("cubicAvailability", Uint8Array::new(f.cubic_availability.clone()))?;
            let chains = f.output_chains.as_ref().map(|chains| {
                let mut object = Object::new(&env)?;
                object.set("children", array(&env, chains.children.iter(), |v| indices(&env, v))?)?;
                object.set("holes", array(&env, chains.holes.iter(), |region|
                    array(&env, region.iter(), |hole|
                        array(&env, hole.iter(), |&(child, edge)| Ok([child as f64, edge as f64]))
                    )
                )?)?;
                Ok::<_, napi::Error>(object)
            }).transpose()?;
            object.set("outputChains", chains)?;
            Ok::<_, napi::Error>(object)
        }).transpose()?;
        object.set("fitting", fitting)?;
    }
    Ok(object)
}

/// Build each JS array directly, without a second Rust tree of copied values.
fn array<'env, T, U: ToNapiValue>(env: &'env Env, values: impl ExactSizeIterator<Item = T>,
    mut convert: impl FnMut(T) -> Result<U>) -> Result<Array<'env>>
{
    let length = u32::try_from(values.len()).map_err(|_| engine_error("Snapshot array exceeds the JavaScript array length limit."))?;
    let mut output = env.create_array(length)?;
    for (index, value) in values.enumerate() { output.set(index as u32, convert(value)?)?; }
    Ok(output)
}

fn indices<'env>(env: &'env Env, values: &[usize]) -> Result<Array<'env>> {
    array(env, values.iter(), |&value| Ok(value as f64))
}

fn contours<'env>(env: Env, geometry: &openvector::contours::Geometry) -> Result<Object<'env>> {
    let nodes = array(&env, geometry.nodes.iter(), |node| {
        let mut object = Object::new(&env)?;
        object.set("x", node.x)?; object.set("y", node.y)?;
        object.set("kind", node.kind as u32)?; object.set("flags", node.flags as u32)?;
        Ok(object)
    })?;
    let regions = array(&env, geometry.regions.iter(), |region| {
        let mut object = Object::new(&env)?;
        object.set("pixelCount", region.pixel_count)?; object.set("colorIndex", region.color_index)?;
        object.set("bgra", region.bgra)?; object.set("parent", region.parent)?;
        object.set("contour", indices(&env, &region.contour)?)?;
        object.set("boundary", &region.boundary)?;
        object.set("neighbors", &region.neighbors)?;
        Ok(object)
    })?;
    let mut object = Object::new(&env)?;
    object.set("nodes", nodes)?; object.set("regions", regions)?;
    Ok(object)
}

fn fitted_geometry<'env>(env: Env, geometry: &openvector::contour_fitting::FittedGeometry) -> Result<Object<'env>> {
    let nodes = array(&env, geometry.nodes.iter(), |node| {
        let mut object = Object::new(&env)?;
        object.set("x", node.x)?; object.set("y", node.y)?;
        object.set("kind", node.kind as u32)?; object.set("cubic", node.cubic)?;
        Ok(object)
    })?;
    let regions = array(&env, geometry.regions.iter(), |region| {
        let mut object = Object::new(&env)?;
        object.set("pixelCount", region.pixel_count)?; object.set("colorIndex", region.color_index)?;
        object.set("bgra", region.bgra)?; object.set("parent", region.parent)?;
        object.set("contour", indices(&env, &region.contour)?)?;
        object.set("edges", &region.edges)?;
        Ok(object)
    })?;
    let cubics = array(&env, geometry.cubics.iter(), |cubic| {
        let mut object = Object::new(&env)?;
        object.set("index", cubic.index as f64)?; object.set("points", cubic.points)?;
        Ok(object)
    })?;
    let mut object = Object::new(&env)?;
    object.set("cubicStorageSlots", geometry.cubic_storage_slots as f64)?;
    object.set("cubics", cubics)?; object.set("nodes", nodes)?; object.set("regions", regions)?;
    Ok(object)
}

pub fn editor_summary(editor: &EditorSnapshot) -> Value {
    json!({"width": editor.width, "height": editor.height, "key": editor.key, "tool": editor.tool,
        "color": editor.color, "undoCount": editor.undo.len(), "redoCount": editor.redo.len()})
}

pub fn editor<'env>(env: Env, editor: EditorSnapshot) -> Result<Object<'env>> {
    let mut object = env.to_js_value(&editor_summary(&editor))?.coerce_to_object()?;
    object.set("bgra", Buffer::from(editor.bgra))?;
    let history = |records: Vec<openvector::segmentation_editor::ToolRecord>| {
        array(&env, records.into_iter(), |record| {
            let mut object = Object::new(&env)?;
            object.set("kind", record.kind)?; object.set("x", record.x)?; object.set("y", record.y)?;
            object.set("coordinates", record.coordinates)?; object.set("argb", record.argb)?;
            object.set("key", record.key)?;
            Ok(object)
        })
    };
    object.set("undo", history(editor.undo)?)?; object.set("redo", history(editor.redo)?)?;
    Ok(object)
}

pub fn background_summary(background: &BackgroundSnapshot) -> Value {
    json!({"count": background.count, "active": background.active, "tool": background.tool,
        "hiddenCount": background.hidden_regions.len(), "undoCount": background.undo.len(), "redoCount": background.redo.len()})
}

pub fn background<'env>(env: Env, background: BackgroundSnapshot) -> Result<Object<'env>> {
    let mut object = env.to_js_value(&background_summary(&background))?.coerce_to_object()?;
    object.set("hiddenRegions", indices(&env, &background.hidden_regions)?)?;
    object.set("undo", indices(&env, &background.undo)?)?; object.set("redo", indices(&env, &background.redo)?)?;
    Ok(object)
}

pub fn queue<'env>(env: Env, queue: openvector::batch_queue::BatchSnapshot) -> Result<Object<'env>> {
    let mut object = Object::new(&env)?;
    object.set("exportDirectory", queue.export_directory)?; object.set("exportFormat", queue.export_format)?;
    object.set("processingIndex", queue.processing_index)?; object.set("acceptingEdits", queue.accepting_edits)?;
    let records = array(&env, queue.queue.into_iter(), |record| {
        let mut object = Object::new(&env)?;
        object.set("filename", record.filename)?; object.set("selected", record.selected)?;
        object.set("outputFilename", record.output_filename)?; object.set("shouldProcess", record.should_process)?;
        object.set("buttonIndex", record.button_index as u32)?;
        Ok(object)
    })?;
    object.set("queue", records)?;
    Ok(object)
}

pub fn batch_result<'env>(env: Env, result: openvector::batch_runner::BatchResult) -> Result<Object<'env>> {
    let mut object = Object::new(&env)?;
    object.set("status", result.status)?; object.set("log", result.log)?;
    object.set("converted", result.converted as u32)?; object.set("failed", result.failed as u32)?;
    object.set("skipped", result.skipped as u32)?; object.set("queue", queue(env, result.queue)?)?;
    let files = array(&env, result.files.into_iter(), |file| {
        let mut object = Object::new(&env)?;
        object.set("filename", file.filename)?; object.set("outputFilename", file.output_filename)?;
        object.set("status", file.status)?;
        if let Some(error) = file.error { object.set("error", error)?; }
        Ok(object)
    })?;
    object.set("files", files)?;
    Ok(object)
}
