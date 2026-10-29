//! Shared helpers for fitting and smoothing differential tests.
#![allow(dead_code)]
use openvector::contours::{Geometry, Node, Region};
use serde_json::Value;

/// `assert.deepEqual` semantics on JSON: numbers compare by f64 value
/// (distinguishing -0 from 0, as `Object.is` does), objects by key set.
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64().unwrap().to_bits() == y.as_f64().unwrap().to_bits(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w))),
        _ => a == b,
    }
}

#[track_caller]
pub fn assert_same(actual: &Value, expected: &Value, message: &str) {
    if !same(actual, expected) {
        let a = actual.to_string(); let e = expected.to_string();
        let at = a.bytes().zip(e.bytes()).position(|(p, q)| p != q).unwrap_or(a.len().min(e.len()));
        let lo = at.saturating_sub(200);
        panic!("{message}\n actual: ...{}\n expect: ...{}", &a[lo..(at + 200).min(a.len())], &e[lo..(at + 200).min(e.len())]);
    }
}

pub fn f(v: &Value) -> f64 { v.as_f64().unwrap() }
pub fn i(v: &Value) -> i64 { v.as_i64().unwrap() }
pub fn f64s(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(f).collect() }
pub fn i32s(v: &Value) -> Vec<i32> { v.as_array().unwrap().iter().map(|x| i(x) as i32).collect() }
pub fn usizes(v: &Value) -> Vec<usize> { v.as_array().unwrap().iter().map(|x| i(x) as usize).collect() }

pub fn node(v: &Value) -> Node {
    Node { x: f(&v["x"]), y: f(&v["y"]), kind: v.get("kind").map_or(0, |k| i(k) as u8), flags: v.get("flags").map_or(0, |k| i(k) as u8) }
}

pub fn region(v: &Value) -> Region {
    let get = |k: &str| v.get(k).filter(|x| !x.is_null());
    let bgra = get("bgra").map(i32s).unwrap_or_default();
    Region {
        pixel_count: get("pixelCount").map_or(0, |x| i(x) as i32),
        color_index: get("colorIndex").map_or(0, |x| i(x) as i32),
        bgra: if bgra.len() == 4 { [bgra[0] as u8, bgra[1] as u8, bgra[2] as u8, bgra[3] as u8] } else { [0; 4] },
        parent: get("parent").map_or(-1, |x| i(x) as i32),
        contour: get("contour").map(usizes).unwrap_or_default(),
        boundary: get("boundary").map(|b| b.as_array().unwrap().iter().map(|r| { let r = i32s(r); [r[0], r[1], r[2]] }).collect()).unwrap_or_default(),
        neighbors: get("neighbors").map(i32s).unwrap_or_default(),
    }
}

pub fn geometry(v: &Value) -> Geometry {
    Geometry { nodes: v["nodes"].as_array().unwrap().iter().map(node).collect(),
        regions: v["regions"].as_array().unwrap().iter().map(region).collect() }
}

/// Owned smoothing fixture state from which a `SmoothingInput` borrows.
pub struct Owned {
    pub geometry: Geometry,
    pub references: Vec<f64>,
    pub penalties: Vec<u8>,
    pub borders: Vec<i64>,
    pub near: Vec<i64>,
    pub direction: Vec<f64>,
    pub parameters: Value,
    pub nearby: Option<Vec<Vec<[usize; 2]>>>,
}

pub fn edges(v: &Value) -> Vec<Vec<[usize; 2]>> {
    v.as_array().unwrap().iter().map(|l| l.as_array().unwrap().iter().map(|e| [i(&e[0]) as usize, i(&e[1]) as usize]).collect()).collect()
}

impl Owned {
    pub fn new(fx: &Value) -> Owned {
        let list = |k: &str| fx.get(k).map(f64s).unwrap_or_default();
        Owned {
            geometry: geometry(&fx["geometry"]),
            references: list("referencePositions"),
            penalties: fx.get("penaltyCounts").map(|p| p.as_array().unwrap().iter().map(|x| i(x) as u8).collect()).unwrap_or_default(),
            borders: fx.get("borderCounts").map(|b| b.as_array().unwrap().iter().map(i).collect()).unwrap_or_default(),
            near: fx.get("nearBorderNodes").map(|b| b.as_array().unwrap().iter().map(i).collect()).unwrap_or_default(),
            direction: list("direction"),
            parameters: fx["parameters"].clone(),
            nearby: fx.get("nearbyEdges").map(edges),
        }
    }
    pub fn input<'a>(&'a self, fx: &Value) -> openvector::smoothing::SmoothingInput<'a> {
        let mut s = openvector::smoothing::SmoothingInput::new(&self.geometry, &self.parameters);
        s.reference_positions = &self.references;
        s.penalty_counts = &self.penalties;
        s.border_counts = &self.borders;
        s.phase = fx.get("phase").map_or(0, i);
        s.width = fx.get("width").map_or(f64::NAN, f);
        s.height = fx.get("height").map_or(f64::NAN, f);
        s.near_border_nodes = &self.near;
        s.direction = &self.direction;
        s.epsilon = fx.get("epsilon").map_or(f64::NAN, f);
        s.nearby_edges = self.nearby.as_deref();
        s.repair_interval = fx.get("repairInterval").map_or(f64::NAN, f);
        s
    }
}

pub fn bytes(v: &Value) -> Vec<u8> {
    v.as_array().unwrap().iter().map(|x| u8::try_from(i(x)).unwrap()).collect()
}

pub fn i64s(v: &Value) -> Vec<i64> { v.as_array().unwrap().iter().map(i).collect() }

pub fn image_model(fx: &Value) -> openvector::image_measurement::ContourImageModel {
    openvector::image_measurement::ContourImageModel::new(
        f(&fx["width"]), f(&fx["height"]), &bytes(&fx["bgra"]),
        &i64s(&fx["segmentation"]["indices"]), &geometry(&fx["geometry"]),
    ).unwrap()
}

pub fn raw_geometry(fx: &Value) -> Geometry {
    let s = &fx["segmentation"];
    openvector::contours::extract_contours(&openvector::contours::ContourInput {
        width: f(&fx["width"]), height: f(&fx["height"]),
        indices: &i64s(&s["indices"]), counts: &i64s(&s["counts"]),
        color_indices: &i64s(&s["colorIndices"]), palette: &f64s(&s["palette"]),
        anti_alias: fx["antiAlias"].as_bool(),
    }).unwrap()
}

pub fn next_uniform(state: &mut i64) -> f64 {
    *state = *state * 2003 % 9973;
    *state as f64 * (1.0 / 9973.0)
}
