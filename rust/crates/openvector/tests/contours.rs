mod common;
use openvector::contours::{extract_contours, ContourInput, Geometry};
use serde_json::Value;

fn ints(v: &Value) -> Vec<i64> { v.as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap()).collect()).unwrap_or_default() }
fn floats(v: &Value) -> Vec<f64> { v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default() }

struct Owned { width: f64, height: f64, indices: Vec<i64>, counts: Vec<i64>, colors: Vec<i64>, palette: Vec<f64>, anti_alias: Option<bool> }
impl Owned {
    fn from(f: &Value) -> Owned {
        Owned { width: f["width"].as_f64().unwrap_or(f64::NAN), height: f["height"].as_f64().unwrap_or(f64::NAN),
            indices: ints(&f["indices"]), counts: ints(&f["counts"]), colors: ints(&f["colorIndices"]), palette: floats(&f["palette"]),
            anti_alias: f["antiAlias"].as_bool() }
    }
    fn input(&self) -> ContourInput<'_> {
        ContourInput { width: self.width, height: self.height, indices: &self.indices, counts: &self.counts,
            color_indices: &self.colors, palette: &self.palette, anti_alias: self.anti_alias }
    }
    fn extract(&self) -> Result<Geometry, openvector::pixel_input::InputError> { extract_contours(&self.input()) }
}

#[test]
fn independent_segmentation_and_contours_match_all_15_original_pipeline_stages() {
    let reference = common::read_json("pipeline-contour-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    for fixture in cases {
        let width = fixture["width"].as_u64().unwrap() as usize;
        let height = fixture["height"].as_u64().unwrap() as usize;
        let bgra: Vec<u8> = fixture["bgra"].as_array().unwrap().iter().map(|v| u8::try_from(v.as_u64().unwrap()).unwrap()).collect();
        let segmented = openvector::segmentation::segment(
            openvector::region_data::Raster { width, height, bgra: &bgra }, &fixture["parameters"], None,
        ).unwrap();
        let actual = extract_contours(&ContourInput {
            width: width as f64, height: height as f64,
            indices: &segmented.indices.iter().map(|&v| v as i64).collect::<Vec<_>>(),
            counts: &segmented.counts.iter().map(|&v| v as i64).collect::<Vec<_>>(),
            color_indices: &segmented.color_indices.iter().map(|&v| v as i64).collect::<Vec<_>>(),
            palette: &segmented.palette.iter().map(|&v| v as f64).collect::<Vec<_>>(),
            anti_alias: Some(fixture["parameters"]["Shared::is_anti_aliased"].as_f64() == Some(1.0)),
        }).unwrap();
        assert_eq!(actual, Geometry::from_json(&fixture["expected"]).unwrap(), "{}", fixture["id"]);
    }
}

#[test]
fn contour_extraction_matches_all_2584_original_ordered_geometry_states() {
    let reference = common::read_json("contour-extraction-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2584);
    let (mut exhaustive, mut staircase, mut nested) = (0, 0, 0);
    for (index, fixture) in cases.iter().enumerate() {
        let expected = Geometry::from_json(&fixture["expected"]).unwrap();
        assert_eq!(Owned::from(fixture).extract().unwrap(), expected, "contour case {index}");
        let purpose = fixture["purpose"].as_str().unwrap_or("");
        if purpose.starts_with("Exhaustive") { exhaustive += 1; }
        if purpose.starts_with("Long") { staircase += 1; }
        if purpose.starts_with("Nested") { nested += 1; }
    }
    assert_eq!((exhaustive, staircase, nested), (1312, 112, 4));
}

#[test]
fn native_geometry_corpus_exercises_staircase_reductions_and_nested_containment() {
    let reference = common::read_json("contour-extraction-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    let starts = |c: &&Value, p: &str| c["purpose"].as_str().is_some_and(|s| s.starts_with(p));
    let staircases: Vec<&Value> = cases.iter().filter(|c| starts(c, "Long")).collect();
    let mut reduced = 0;
    for pair in staircases.chunks(2) {
        let (plain, blended) = (pair[0], pair[1]);
        assert_eq!(plain["indices"], blended["indices"]);
        assert_eq!(plain["antiAlias"].as_bool(), Some(false));
        assert_eq!(blended["antiAlias"].as_bool(), Some(true));
        if plain["expected"]["nodes"].as_array().unwrap().len() < blended["expected"]["nodes"].as_array().unwrap().len() { reduced += 1; }
    }
    assert!(reduced > 0, "Plain-image constructor patterns must actually remove samples.");
    for fixture in cases.iter().filter(|c| starts(c, "Nested")) {
        let parents: Vec<i64> = fixture["expected"]["regions"].as_array().unwrap().iter().map(|r| r["parent"].as_i64().unwrap()).collect();
        assert_eq!(parents, vec![-1, 0, 1, 2]);
    }
}

#[test]
fn contours_own_all_output_storage_and_preserve_typed_input_arrays() {
    let reference = common::read_json("contour-extraction-reference.json");
    let fixture = reference["cases"].as_array().unwrap().iter()
        .find(|c| c["purpose"].as_str().is_some_and(|s| s.starts_with("Nested"))).unwrap();
    let input = Owned::from(fixture);
    let before = (input.indices.clone(), input.counts.clone(), input.colors.clone(), input.palette.clone());
    let mut geometry = input.extract().unwrap();
    for node in geometry.nodes.iter_mut() { node.x = 7.0; node.flags = 7; }
    for region in geometry.regions.iter_mut() {
        region.contour.fill(7); region.neighbors.fill(7); region.bgra.fill(7);
        for edge in region.boundary.iter_mut() { edge.fill(7); }
    }
    assert_eq!((input.indices.clone(), input.counts.clone(), input.colors.clone(), input.palette.clone()), before);
    assert_eq!(input.extract().unwrap(), Geometry::from_json(&fixture["expected"]).unwrap());
}

#[test]
fn contour_input_validation_rejects_inconsistent_or_disconnected_region_maps() {
    let reference = common::read_json("contour-extraction-reference.json");
    let input = &reference["cases"][0];
    let with = |pairs: &[(&str, Value)]| { let mut v = input.clone(); for (k, x) in pairs { v[*k] = x.clone(); } v };
    use serde_json::json;
    let disconnected = [("indices", json!([0, 1, 1, 0])), ("counts", json!([2, 2])), ("colorIndices", json!([0, 0]))];
    // `counts: new Array(1)` (a hole) has no typed Rust form and is omitted.
    let invalid = [json!(null), json!({}), with(&[("width", json!(1))]), with(&[("height", json!(2.5))]),
        with(&[("antiAlias", Value::Null)]), with(&[("antiAlias", json!(1))]), with(&[("indices", json!([0]))]),
        with(&[("indices", json!([0, 0, 0, -1]))]), with(&[("indices", json!([1, 1, 1, 1]))]),
        with(&[("counts", json!([3]))]), with(&[("counts", json!([4, 0])), ("colorIndices", json!([0, 0]))]),
        with(&[("colorIndices", json!([1]))]), with(&[("palette", json!([]))]), with(&[("palette", json!([null, 0, 0, 1]))]),
        with(&[disconnected[0].clone(), disconnected[1].clone(), disconnected[2].clone(), ("indices", json!([1, 0, 0, 1]))]),
        with(&disconnected)];
    for case in &invalid { assert!(Owned::from(case).extract().is_err(), "{case}"); }
}

#[test]
fn geometry_json_rejects_lossy_numeric_identifiers_and_byte_values() {
    let reference = common::read_json("contour-extraction-reference.json");
    let base = &reference["cases"][0]["expected"];
    for value in [serde_json::json!(-1), serde_json::json!(256), serde_json::json!(1.5)] {
        let mut invalid = base.clone();
        invalid["nodes"][0]["flags"] = value;
        assert!(Geometry::from_json(&invalid).is_none());
    }
    for value in [serde_json::json!(-1), serde_json::json!(1.5), serde_json::json!("0")] {
        let mut invalid = base.clone();
        invalid["regions"][0]["contour"][0] = value;
        assert!(Geometry::from_json(&invalid).is_none());
    }
    let decoded = Geometry::from_json(base).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), decoded.to_json());
}
