//! Fixture translation of the JavaScript fitted-geometry object shape
//! (`{cubicStorageSlots, cubics, nodes, regions}`) into `FittedGeometry`.
#![allow(dead_code)]
use openvector::contour_fitting::FittedGeometry;
use serde_json::Value;


pub fn fitted_geometry(value: &Value) -> FittedGeometry {
    FittedGeometry::from_json(value).unwrap()
}
