//! Fresh original captures from the 2026-10-22 full-parity captures lane.
//! Every case ran in two fresh original processes whose complete raw responses are
//! byte-identical (evidence/full-parity-program-20261022/captures/).
//!
//! The recorded public wizard actions are replayed against the shared engine: the
//! user's control choices come from the original's selected settings before each
//! Next, while every result the original computed (classification, palette search
//! and candidate choice, segmentation, contours, smoothing, fitted geometry, random
//! state, stage keys and the selected palette/counts) is computed by Rust and compared
//! exactly at every processed frame.
mod common;
mod editor_support;

use editor_support::{clean_startup_profile, compare_document_state, compare_settings, hex};
use openvector::document_controls::DocumentSettings;
use openvector::image_document::ImageDocument;
use openvector::palette::CoreSession;
use openvector::profile_palette::{PaletteCommand, PaletteProfile, PaletteRuntime, apply_palette_command};
use openvector::saved_profiles::RecentBasicProfiles;
use openvector::settings_profile::{SettingsProfile, basic_profile_label, run_basic_profile};
use serde_json::{Value, json};
use std::sync::LazyLock;

fn rows() -> &'static [Value] {
    static ROWS: LazyLock<Vec<Value>> = LazyLock::new(|| common::read_ndjson("fresh-captures-20261022.ndjson.gz"));
    &ROWS
}

fn parameters() -> &'static Value { &rows()[0]["parameters"] }

fn row(id: &str) -> &'static Value {
    let row = rows().iter().find(|row| row["id"] == id).unwrap_or_else(|| panic!("{id}: missing capture"));
    let runs = row["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "{id}: two original processes");
    assert_ne!(runs[0]["pid"], runs[1]["pid"], "{id}: independent processes");
    assert_eq!(runs[0]["responseSha256"], runs[1]["responseSha256"], "{id}: raw responses repeat");
    assert_eq!(row["rawPairIdentical"], true);
    row
}

fn i32_at(bits: &[u8], offset: usize) -> i32 { i32::from_le_bytes(bits[offset..offset + 4].try_into().unwrap()) }

/// Inverse of editor_support::compare_settings for the 92-byte selected settings block.
/// The palette colors are not part of the block; the caller supplies them.
fn settings_from_bits(bits: &str, palette: Vec<f32>) -> DocumentSettings {
    let b = hex(bits);
    let f32_at = |offset: usize| f32::from_le_bytes(b[offset..offset + 4].try_into().unwrap());
    DocumentSettings {
        category: i32_at(&b, 0), quality: i32_at(&b, 4), sensitivity: i32_at(&b, 12), anti_alias: i32_at(&b, 16) != 0,
        complexity: i32_at(&b, 20), minimum_pixels: i32_at(&b, 24), artifact_rejection: i32_at(&b, 28),
        cluster_colors: i32_at(&b, 32) != 0, smoothness: i32_at(&b, 36), detect_corners: i32_at(&b, 40) != 0,
        curve_complexity: i32_at(&b, 44), smoothing_anti_alias: i32_at(&b, 48) != 0, advanced: i32_at(&b, 52) != 0,
        palette_mode: i32_at(&b, 60), background_mode: i32_at(&b, 68),
        background: [f32_at(84), f32_at(80), f32_at(76), f32_at(72)], palette,
    }
}

/// Load a captured source exactly as the recorder staged it (`input.png`/`replacement.png`).
fn load(document: &mut ImageDocument, input: &Value, staged_name: &str) {
    let decoded = openvector::image_input::decode_image(&common::b64(input["bytes"].as_str().unwrap())).unwrap();
    let filename = openvector::image_input::resolve_image_filename(staged_name);
    document.set_source(decoded.width, decoded.height, &decoded.bgra, decoded.has_alpha, filename.shadow_control as f64).unwrap();
}

fn controls(settings: &DocumentSettings) -> Value {
    let mut value = Value::Object(settings.to_json());
    value["extension"] = json!(".png");
    value
}

/// The wizard's selected profile: settings plus the counters stored beside them.
#[derive(Clone)]
struct Selection { settings: DocumentSettings, color_count: i32, palette_color_count: i32 }

impl Selection {
    fn profile(&self, bits: &str) -> SettingsProfile {
        let b = hex(bits);
        SettingsProfile { settings: self.settings.clone(), color_count: self.color_count, basic_color_count: i32_at(&b, 64),
            palette_color_count: self.palette_color_count, recommended_background_mode: i32_at(&b, 88) }
    }
    /// Exact selected controls/counts/palette against the original's selection at a frame.
    fn compare(&self, frame: &Value, label: &str) {
        let bits = hex(frame["selectedSettingsBits"].as_str().unwrap());
        assert_eq!((i32_at(&bits, 8), i32_at(&bits, 56)), (self.color_count, self.palette_color_count), "{label}: selected color/palette counts");
        let palette_bits = frame["processed"]["selectedPaletteArgbBits"].as_str().unwrap_or("");
        if frame.get("processed").is_some() {
            compare_settings(&self.settings, frame["selectedSettingsBits"].as_str().unwrap(), palette_bits);
        }
    }
}

/// Basic palette search: Two colors searches 2..2, a fixed palette 2..12. The lowest
/// score wins and ties keep the smaller count (worker `process`, native page 10/11).
fn basic_palette(document: &mut ImageDocument, selection: &mut Selection) {
    let upper = if selection.settings.palette_mode == 0 { 2 } else { 12 };
    let mut input = controls(&selection.settings);
    input["lower"] = json!(2); input["upper"] = json!(upper);
    let result = document.run_task("palette", &input, None).unwrap();
    assert_eq!(result.state.status, 0);
    let mut chosen = 2;
    for count in 3..=upper { if result.state.scores[count] < result.state.scores[chosen] { chosen = count; } }
    choose(document, selection, chosen);
}

fn choose(document: &ImageDocument, selection: &mut Selection, count: usize) {
    let palette: Vec<f64> = document.state().palettes[count].chunks_exact(4)
        .flat_map(|color| color.iter().rev().map(|&v| v as f64)).collect();
    palette_command(selection, PaletteCommand::Choose(palette));
}

fn palette_command(selection: &mut Selection, command: PaletteCommand) {
    let (mut color_count, mut palette_count) = (selection.color_count as f64, selection.palette_color_count as f64);
    apply_palette_command(&mut PaletteProfile { color_count: &mut color_count, palette_color_count: &mut palette_count,
        palette: &mut selection.settings.palette }, &mut PaletteRuntime::default(), &command).unwrap();
    selection.color_count = color_count as i32; selection.palette_color_count = palette_count as i32;
}

/// Segmentation, smoothing and fitting with the selected controls. Returns fitting did_work.
fn vectorize(document: &mut ImageDocument, selection: &Selection) -> bool {
    let mut did_work = false;
    for stage in ["segmentation", "smoothing", "fitting"] {
        let result = document.run_task(stage, &controls(&selection.settings), None).unwrap();
        assert_eq!(result.state.status, 0, "{stage}");
        did_work = result.did_work;
    }
    did_work
}

fn compare_state(document: &ImageDocument, row: &Value, frame: &Value, label: &str) {
    let expected = &row["states"][frame["processed"]["stateSha256"].as_str().unwrap()];
    let state = &document.snapshot();
    let classification = &state.classification;
    if classification.category != -1 {
        assert_eq!(json!([classification.category, classification.quality, classification.palette_mode, classification.color_count]),
            frame["processed"]["documentClassification"], "{label}: classification");
    }
    compare_document_state(state, expected, parameters());
}

/// Replays one recorded session. Pages follow native WizardPageIdType: 4 source/mode,
/// 5 image type, 6 transparency, 7..9 quality, 10 colors, 11 palette, 19 review.
fn replay_session(id: &str) -> (ImageDocument, RecentBasicProfiles) {
    let row = row(id);
    let mut document = CoreSession::default().create_document();
    load(&mut document, &row["input"], "input.png");
    let loaded = row["loadedSelectedSettingsBits"].as_str().unwrap();
    let mut selection = Selection { settings: settings_from_bits(loaded, Vec::new()), color_count: i32_at(&hex(loaded), 8),
        palette_color_count: i32_at(&hex(loaded), 56) };
    let mut recent = RecentBasicProfiles::default();
    let (mut page, mut classified, mut previous_bits) = (4, false, loaded.to_owned());
    let mut mismatches = Vec::new();
    for (index, (frame, action)) in row["frames"].as_array().unwrap().iter().zip(row["actions"].as_array().unwrap()).enumerate() {
        let label = format!("{id} frame {index} action {action}");
        let bits = frame["selectedSettingsBits"].as_str().unwrap();
        let next_page = frame["page"].as_i64().unwrap();
        match (action["action"].as_i64().unwrap(), action["slot"].as_str().unwrap_or("")) {
            (23, _) => {
                // The user's choices are the selection before Next; results are Rust's.
                let palette = std::mem::take(&mut selection.settings.palette);
                selection.settings = settings_from_bits(&previous_bits, palette);
                match (page, next_page) {
                    (4, 5) if !classified => {
                        document.shrink_source();
                        let mut input = controls(&selection.settings);
                        if index == 0 { input = clean_startup_profile(); input["extension"] = json!(".png"); }
                        let result = document.run_task("classification", &input, None).unwrap();
                        if std::env::var_os("OPENVECTOR_CLASSIFICATION_DIAGNOSTICS").is_some() {
                            let expected = &row["states"][frame["processed"]["stateSha256"].as_str().unwrap()];
                            let expected_indices = expected["segmentation"]["indices"].as_array().unwrap();
                            let analysis = result.analysis.as_ref().unwrap();
                            eprintln!("CLASSIFICATION {label}: random={} input={input}", result.state.random_state);
                            for candidate in &analysis.palette_analysis.candidates {
                                let differences: Vec<_> = candidate.generated.indices.iter().zip(expected_indices).enumerate()
                                    .filter(|(_, (actual, expected))| i64::from(**actual) != expected.as_i64().unwrap())
                                    .map(|(pixel, (actual, expected))| (pixel, *actual, expected.as_i64().unwrap()))
                                    .collect();
                                eprintln!("CANDIDATE {label}: count={} passes={} random={} differences={} first={:?} palette={:?} expected_palette={}",
                                    candidate.color_count, candidate.generated.passes, candidate.random_state,
                                    differences.len(), &differences[..differences.len().min(16)],
                                    candidate.generated.palette, expected["segmentation"]["palette"]);
                            }
                        }
                        let classification = &document.state().classification;
                        selection.settings.category = classification.category;
                        selection.settings.quality = classification.quality;
                        selection.settings.palette_mode = classification.palette_mode;
                        classified = true;
                    }
                    (5, 6) => {
                        let flatten = selection.settings.category == 1 && document.source_alpha_type() == 1;
                        selection.settings.background_mode = i32::from(flatten);
                        selection.settings.background = [1.0, 1.0, 1.0, i32::from(flatten) as f32];
                    }
                    (10, 11) => basic_palette(&mut document, &mut selection),
                    (10, 19) | (11, 19) => {
                        if page == 10 && matches!(selection.settings.palette_mode, 0 | 1) { basic_palette(&mut document, &mut selection); }
                        let did_work = vectorize(&mut document, &selection);
                        let fitted = document.state().fitting.as_ref().is_some_and(|f| f.complete);
                        assert!(fitted, "{label}: Review needs a completed fitting");
                        recent.record_completion(&selection.profile(bits), 0, did_work, false);
                    }
                    (10, 10) => assert!(!(0..=2).contains(&selection.settings.palette_mode),
                        "{label}: Next stays on Colors only for an unrecognized palette mode"),
                    _ => {}
                }
            }
            (19, "setPaletteFromList(int)") => {
                // Retained native row 6 selects seven colors (selected counts 7/7).
                choose(&document, &mut selection, action["value"].as_u64().unwrap() as usize + 1);
            }
            (19, "deleteColor(int)") => palette_command(&mut selection, PaletteCommand::Delete(action["value"].as_f64().unwrap())),
            (19, "addColor()") => palette_command(&mut selection, PaletteCommand::Add),
            (29, "vectorizeImageWithMruSettings(int)") => {
                let stored = recent.entries()[action["value"].as_u64().unwrap() as usize].profile.clone();
                let result = run_basic_profile(&mut document, &stored, &PaletteRuntime::default(), None, None).unwrap()
                    .expect("recorded entries always have a processing palette mode");
                assert_eq!(next_page, 19, "{label}: replay completes on Review");
                selection = Selection { settings: result.profile.settings.clone(), color_count: result.profile.color_count,
                    palette_color_count: result.profile.palette_color_count };
                let fitting = result.did_work["fitting"];
                recent.record_completion(&result.profile, 0, fitting, false);
            }
            (44, _) => {
                load(&mut document, &row["replacement"], "replacement.png");
                selection.settings.palette.clear();
                classified = false;
            }
            (19, _) | (29, _) | (32, _) => {}
            other => panic!("{label}: unexpected action {other:?}"),
        }
        let mut check = |kind: &str, compare: &mut dyn FnMut()| {
            eprintln!("CHECK {label}: {kind}");
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(compare)).is_err() {
                mismatches.push(format!("{label}: {kind}"));
            }
        };
        if frame.get("processed").is_some() {
            check("selection", &mut || selection.compare(frame, &label));
            check("document", &mut || compare_state(&document, row, frame, &label));
        } else if action["action"] == 19 && action["slot"].as_str().is_some_and(|s| s.contains("Color") || s.contains("Palette")) {
            check("selection", &mut || selection.compare(frame, &label));
        }
        page = next_page; previous_bits = bits.to_owned();
    }
    assert!(mismatches.is_empty(), "{id}: exact comparisons failed: {mismatches:?}");
    (document, recent)
}

fn compare_automatic(id: &str) {
    let row = row(id);
    let frame = row["frames"].as_array().unwrap().iter().find(|frame| frame.get("processed").is_some()).unwrap();
    let (_, automatic) = editor_support::automatic_document(&row["input"]);
    editor_support::compare_settings(&automatic.settings, frame["processed"]["selectedSettingsBits"].as_str().unwrap(),
        frame["processed"]["selectedPaletteArgbBits"].as_str().unwrap());
    let c = &automatic.state.classification;
    assert_eq!(json!([c.category, c.quality, c.palette_mode, c.color_count]), frame["processed"]["documentClassification"]);
    assert_eq!(json!([c.category, c.quality, c.palette_mode]), frame["processed"]["recommendedControls"]);
    compare_document_state(&automatic.state, &row["states"][frame["processed"]["stateSha256"].as_str().unwrap()], parameters());
}

/// (h) Further fitting/classification fresh inputs: public-domain photographs, Fully Automatic.
mod photographic_automatic {
    #[test] fn brown_pelican() { super::compare_automatic("automatic-photo-brown-pelican"); }
    #[test] fn american_bison() { super::compare_automatic("automatic-photo-american-bison"); }
    #[test] fn arctic_fox_crop() { super::compare_automatic("automatic-photo-arctic-fox-crop"); }
}

/// (g) Basic wizard: orthogonal L9 over category x quality x palette mode x input,
/// plus two transparency-page sessions.
mod basic_wizard {
    #[test] fn l9_row1() { super::replay_session("basic-l9-row1"); }
    #[test] fn l9_row2() { super::replay_session("basic-l9-row2"); }
    #[test] fn l9_row3() { super::replay_session("basic-l9-row3"); }
    #[test] fn l9_row4() { super::replay_session("basic-l9-row4"); }
    #[test] fn l9_row5() { super::replay_session("basic-l9-row5"); }
    #[test] fn l9_row6() { super::replay_session("basic-l9-row6"); }
    #[test] fn l9_row7_portrait_52x64() { super::replay_session("basic-l9-row7-portrait-52x64"); }
    #[test] fn l9_row8() { super::replay_session("basic-l9-row8"); }
    #[test] fn l9_row9() { super::replay_session("basic-l9-row9"); }
    #[test] fn alpha_rings_antialiased_medium_fixed() { super::replay_session("basic-alpha-rings-c1q1p1"); }
    #[test] fn alpha_rings_artwork_low_two_colors() { super::replay_session("basic-alpha-rings-c2q2p0"); }
}

/// (b) Portrait photograph through Basic to Review at High and Low quality.
mod review_portrait {
    #[test] fn photograph_high() { super::replay_session("review-portrait-photo-high"); }
    #[test] fn photograph_low() { super::replay_session("review-portrait-photo-low"); }
}

/// (a) Accumulated palette-page session: list choice, delete, add, delete, then Review.
#[test]
fn accumulated_palette_session() { replay_session("palette-session-basic"); }

/// (e) A completed Basic run, then a genuine second source load and a new run.
mod cache_invalidation {
    #[test] fn replacement_after_completed_processing() { super::replay_session("cache-replace-completed"); }
    /// The replacement is reduced by default optional sizing (1728x1152 -> 864x576).
    #[test]
    fn shrinking_replacement_after_completed_processing() {
        let (document, _) = super::replay_session("cache-replace-completed-shrink-two-colors");
        assert_eq!([document.state().original.width, document.state().original.height], [864, 576]);
    }
}

/// (d) Recent Basic Settings with real original recording and replay.
mod recent_basic {
    use super::*;
    /// Five distinct labels: the oldest is stored at index 4 (hidden in the view, yet the
    /// public replay slot accepts it); replaying it moves it to the front.
    #[test]
    fn five_entries_hidden_fifth_and_replay_navigation() {
        let (_, recent) = replay_session("recent-basic-five-hidden");
        let labels: Vec<String> = recent.entries().iter().map(|e| e.label.clone()).collect();
        assert_eq!(labels, ["Artwork, Low, Two colors", "Artwork, High, Two colors", "Photograph, Low, Unlimited colors",
            "Anti-aliased artwork, High, 5 colors", "Artwork, Medium, Unlimited colors"]);
        assert_eq!(recent.visible_entries().len(), 4);
    }
    /// The same label with a different flattening mode keeps the older stored settings.
    #[test]
    fn duplicate_label_keeps_older_stored_settings() {
        let (_, recent) = replay_session("recent-basic-duplicate");
        assert_eq!(recent.entries().len(), 1);
        assert_eq!(recent.entries()[0].profile.settings.background_mode, -1);
        assert_eq!(basic_profile_label(&recent.entries()[0].profile), "Artwork, Medium, Unlimited colors");
    }
    /// Next with palette mode 3 stays on Colors without processing, so nothing is recorded:
    /// a real workflow cannot store an unrecognized mode for apply-only replay.
    #[test]
    fn unrecognized_mode_is_never_recorded() {
        let (_, recent) = replay_session("recent-basic-unrecognized-mode");
        assert_eq!(recent.entries().len(), 1);
        assert_eq!(recent.entries()[0].profile.settings.palette_mode, 0);
    }
}

/// (f) Large-image Fully Automatic: 3600x2400 artwork with default optional sizing
/// (reduced to 1200x800 when leaving Source) and with sizing disabled (full size).
/// Rust runs to completion without a deadline; wall times are printed beside the
/// original process times for the evidence record.
mod large_automatic {
    use super::*;
    fn compare(id: &str, shrink: bool) {
        let rows = common::read_ndjson("fresh-large-automatic-20261022.ndjson.gz");
        let row = rows.iter().find(|row| row["id"] == id).unwrap();
        assert_eq!(row["runs"][0]["responseSha256"], row["runs"][1]["responseSha256"]);
        let frame = row["frames"].as_array().unwrap().iter().find(|frame| frame.get("processed").is_some()).unwrap();
        let expected = &row["states"][frame["processed"]["stateSha256"].as_str().unwrap()];
        let mut document = CoreSession::default().create_document();
        load(&mut document, &row["input"], "input.png");
        let started = std::time::Instant::now();
        // The full-size capture disables optional sizing before leaving Source.
        if shrink { assert!(document.shrink_source(), "{id}: optional sizing"); }
        let mut input = clean_startup_profile(); input["extension"] = json!(".png");
        let automatic = document.run_automatic(&input).unwrap();
        let elapsed = started.elapsed().as_secs_f64();
        eprintln!("{id}: Rust automatic {elapsed:.3} s; original processes {} s and {} s (launch, load, automatic and capture)",
            row["runs"][0]["elapsedSeconds"], row["runs"][1]["elapsedSeconds"]);
        compare_settings(&automatic.settings, frame["processed"]["selectedSettingsBits"].as_str().unwrap(),
            frame["processed"]["selectedPaletteArgbBits"].as_str().unwrap());
        let c = &automatic.state.classification;
        assert_eq!(json!([c.category, c.quality, c.palette_mode, c.color_count]), frame["processed"]["documentClassification"]);
        compare_document_state(&automatic.state, expected, &rows[0]["parameters"]);
    }
    #[test] fn default_sizing_3600x2400() { compare("large-automatic-default-sizing", true); }
    #[test] fn full_size_3600x2400() { compare("large-automatic-full-size", false); }
}
