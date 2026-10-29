mod common;
mod smoothing_support;
use openvector::contour_initialization::{initialize_contour_coordinates, preadjust_contours, prepare_contour_smoothing, select_near_border_nodes, InitializationInput};
use serde_json::{json, Value};
use smoothing_support::*;
use std::collections::BTreeSet;

struct InitializationFixture {
    owned: Owned,
    bgra: Vec<u8>,
    indices: Vec<i64>,
    palette: Vec<f64>,
}
impl InitializationFixture {
    fn new(fx: &Value) -> Self {
        Self { owned: Owned::new(fx), bgra: bytes(&fx["bgra"]), indices: i64s(&fx["segmentation"]["indices"]), palette: f64s(&fx["segmentation"]["palette"]) }
    }
    fn input(&self, fx: &Value) -> InitializationInput<'_> {
        InitializationInput { width: i(&fx["width"]), height: i(&fx["height"]), bgra: &self.bgra, geometry: &self.owned.geometry,
            indices: &self.indices, reference_positions: &self.owned.references, border_counts: &self.owned.borders,
            parameters: &self.owned.parameters, palette: &self.palette }
    }
}

#[test]
fn preparation_matches_all_2700_native_geometries_and_fixed_caches() {
    let reference = common::read_json("contour-preparation-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2700);
    for (index, fx) in cases.iter().enumerate() {
        let raw = raw_geometry(fx);
        let actual = prepare_contour_smoothing(i(&fx["width"]), i(&fx["height"]), fx["antiAlias"].as_bool(), &raw).unwrap();
        assert_same(&actual.to_json(), &fx["expected"], &format!("preparation {index}"));
    }
}

#[test]
fn near_border_selection_preserves_all_96_threshold_adjacent_probes() {
    let reference = common::read_json("contour-preparation-reference.json");
    let (mut probes, mut remainders) = (0, BTreeSet::new());
    for (index, fx) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let g = geometry(&fx["geometry"]);
        let borders = i64s(&fx["borderCounts"]);
        let actual = select_near_border_nodes(i(&fx["width"]), i(&fx["height"]), &g, &borders).unwrap();
        assert_same(&json!(actual), &fx["expectedNearBorderNodes"], &format!("near-border {index}"));
        if let Some(probe) = fx.get("thresholdProbe") {
            if probe["precision"] == "float32" { assert_eq!(f(&probe["value"]) as f32 as f64, f(&probe["value"])); }
            probes += 1;
            remainders.insert((g.nodes.len() - borders.iter().sum::<i64>() as usize) % 4);
        }
    }
    assert_eq!(probes, 96);
    assert_eq!(remainders.into_iter().collect::<Vec<_>>(), vec![0, 1, 2, 3]);
}

#[test]
fn preadjustment_matches_all_2604_native_position_arrays_without_randomness() {
    let reference = common::read_json("smoothing-preadjustment-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2604);
    let mut moved = 0;
    for (index, fx) in cases.iter().enumerate() {
        let owned = InitializationFixture::new(fx);
        let positions = preadjust_contours(&owned.input(fx)).unwrap();
        assert_same(&json!(positions), &fx["expected"]["positions"], &format!("preadjustment {index}"));
        assert_eq!(i(&fx["randomStateBefore"]), 1);
        assert_eq!(i(&fx["expected"]["randomState"]), 1);
        moved += owned.owned.geometry.nodes.iter().enumerate().filter(|(n, p)| positions[*n * 2] != p.x || positions[*n * 2 + 1] != p.y).count();
    }
    assert_eq!(moved, 37310);
}

#[test]
fn initialization_matches_all_2604_calls_in_one_retained_random_stream() {
    let reference = common::read_json("smoothing-initialization-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2604);
    let (mut state, mut draws, mut plain, mut zero_range) = (1, 0, 0, 0);
    for (index, fx) in cases.iter().enumerate() {
        assert_eq!(state, i(&fx["randomStateBefore"]), "random boundary {index}");
        let owned = InitializationFixture::new(fx);
        let positions = initialize_contour_coordinates(&owned.input(fx), &mut || { draws += 1; next_uniform(&mut state) }).unwrap();
        assert_same(&json!({"positions": positions, "randomState": state}), &fx["expected"], &format!("initialization {index}"));
        let border = owned.owned.borders.iter().sum::<i64>() as usize;
        for (node, p) in owned.owned.geometry.nodes[..border].iter().enumerate() { assert_eq!([positions[node * 2], positions[node * 2 + 1]], [p.x, p.y]); }
        if fx["antiAlias"] == true {
            if fx["amplitudeVariant"] == 0 && state != i(&fx["randomStateBefore"]) { zero_range += 1; }
        } else {
            plain += 1;
            assert_eq!(state, i(&fx["randomStateBefore"]));
            assert_eq!(positions, owned.owned.geometry.nodes.iter().flat_map(|p| [p.x, p.y]).collect::<Vec<_>>());
        }
    }
    assert_eq!((draws, plain, zero_range, state), (111576, 1157, 4, 8900));
}

#[test]
fn independent_preparation_composes_with_all_2604_retained_initializations() {
    let reference = common::read_json("smoothing-initialization-reference.json");
    let mut state = 1;
    for (index, fx) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let prepared = prepare_contour_smoothing(i(&fx["width"]), i(&fx["height"]), fx["antiAlias"].as_bool(), &raw_geometry(fx)).unwrap();
        let mut owned = InitializationFixture::new(fx);
        owned.owned.geometry = prepared.geometry;
        owned.owned.references = prepared.reference_positions.iter().map(|&v| v as f64).collect();
        owned.owned.borders = prepared.border_counts.to_vec();
        let positions = initialize_contour_coordinates(&owned.input(fx), &mut || next_uniform(&mut state)).unwrap();
        assert_same(&json!({"positions": positions, "randomState": state}), &fx["expected"], &format!("prepared initialization {index}"));
    }
}

#[test]
fn palette_generation_and_initialization_share_all_110_native_interleaved_runs() {
    let reference = common::read_json("smoothing-initialization-mixed-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 110);
    let session = openvector::palette::CoreSession::default();
    for (index, fx) in cases.iter().enumerate() {
        assert_eq!(session.random_state() as i64, i(&fx["warmupStateBefore"]));
        let warmup = &fx["paletteWarmup"];
        let palette = session.generate_unweighted(&bytes(&warmup["bgra"]), i(&warmup["colorCount"]) as usize,
            i(&warmup["bins"]) as usize, warmup["connected"].as_bool().unwrap()).unwrap();
        assert_same(&json!({"palette": palette.palette, "indices": palette.indices, "weights": palette.weights,
            "randomState": session.random_state()}), &fx["warmupExpected"], &format!("palette warmup {index}"));
        assert_eq!(session.random_state() as i64, i(&fx["randomStateBefore"]));
        let owned = InitializationFixture::new(fx);
        let positions = initialize_contour_coordinates(&owned.input(fx), &mut || session.next_random()).unwrap();
        assert_same(&json!({"positions": positions, "randomState": session.random_state()}), &fx["expected"], &format!("mixed initialization {index}"));
    }
    assert_eq!(session.random_state(), 5295);
}

#[test]
fn preparation_owns_storage_and_rejects_moved_or_misordered_grid_nodes() {
    let reference = common::read_json("contour-preparation-reference.json");
    let fx = reference["cases"].as_array().unwrap().iter().find(|f| f["antiAlias"] == true && f["geometry"]["nodes"].as_array().unwrap().len() > 10).unwrap();
    let raw = raw_geometry(fx);
    let before = raw.clone();
    let (width, height) = (i(&fx["width"]), i(&fx["height"]));
    let mut actual = prepare_contour_smoothing(width, height, Some(true), &raw).unwrap();
    actual.geometry.nodes[0].x = 999.0;
    actual.reference_positions.fill(999.0); actual.penalty_counts.fill(255); actual.border_counts.fill(999); actual.near_border_nodes.fill(999);
    actual.nearby_edges.as_mut().unwrap()[0].push([999, 999]);
    assert_eq!(raw, before);
    let mut moved = raw.clone(); moved.nodes.last_mut().unwrap().x += 0.125;
    let mut misordered = raw.clone(); let last = misordered.nodes.len() - 1; misordered.nodes.swap(0, last);
    assert!(prepare_contour_smoothing(width, height, None, &raw).is_err());
    assert!(prepare_contour_smoothing(1, height, Some(true), &raw).is_err());
    for g in [&moved, &misordered] { assert!(prepare_contour_smoothing(width, height, Some(true), g).is_err()); }
    for b in [[2, 2, 999, 0], [2, 2, -1, 0]] { assert!(select_near_border_nodes(width, height, &raw, &b).is_err()); }
}

#[test]
fn initialization_rejects_invalid_inputs_before_consuming_randomness() {
    let reference = common::read_json("smoothing-initialization-reference.json");
    let fx = reference["cases"].as_array().unwrap().iter().find(|f| f["antiAlias"] == true && f["geometry"]["nodes"].as_array().unwrap().len() > i64s(&f["borderCounts"]).iter().sum::<i64>() as usize).unwrap();
    let mut state = 1;
    for variant in 0..8 {
        let mut owned = InitializationFixture::new(fx);
        match variant {
            0 => owned.owned.references.fill(0.0),
            1 => owned.owned.references.fill(f64::INFINITY),
            2 => owned.owned.borders = vec![2, 2, 0, 999999],
            3 => owned.indices.fill(owned.owned.geometry.regions.len() as i64),
            4 => owned.palette = vec![f64::NAN, 0.0, 0.0, 1.0],
            5 => owned.owned.geometry.nodes[0].x = f64::INFINITY,
            6 => owned.owned.parameters["ContourSmoother::perturbation_range"] = json!(-0.1),
            7 => owned.owned.parameters["Shared::is_anti_aliased"] = json!(2),
            _ => unreachable!(),
        }
        assert!(initialize_contour_coordinates(&owned.input(fx), &mut || next_uniform(&mut state)).is_err());
        assert_eq!(state, 1);
    }
    let owned = InitializationFixture::new(fx);
    let before = owned.owned.geometry.clone();
    initialize_contour_coordinates(&owned.input(fx), &mut || next_uniform(&mut state)).unwrap().fill(999.0);
    assert_eq!(owned.owned.geometry, before);
    assert_ne!(state, 1);
}
