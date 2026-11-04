#[path = "common/mod.rs"]
mod common;

use openvector::contour_fitting::FittedGeometry;
use openvector::vector_file::{write_vector_file_admitted, write_vector_file_safely, VectorFileInput};
use openvector::vector_output::VectorOutputInput;

#[test]
fn safe_vector_output_matches_existing_serializer_bytes() {
    let fixture = common::read_json("pdf-cli-reference.json");
    let row = &fixture["cases"][0]["input"];
    let geometry = FittedGeometry::from_json(&row["geometry"]).unwrap();
    let vector = VectorOutputInput::new(&geometry, row["width"].as_i64().unwrap(), row["height"].as_i64().unwrap());
    let mut input = VectorFileInput::new(vector);
    input.eps.creation_date = Some("Tue Oct 20 12:34:56 2026".into());
    input.palette = &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0];
    input.reference_device = Some(openvector::emf::EmfDevice { pixels: [1920, 1080], millimeters: [508, 286], micrometers: [508000, 286000] });
    let directory = tempfile::tempdir().unwrap();
    for format in ["svg", "pdf", "eps", "ai", "dxf", "emf"] {
        let legacy = directory.path().join(format!("legacy.{format}"));
        let safe = directory.path().join(format!("safe.{format}"));
        let mut progress = |_| Ok(());
        let expected = write_vector_file_admitted(&legacy, &input, false, &mut progress).unwrap();
        assert_eq!(expected.status, 0);
        assert!(expected.error.is_none());
        let actual = write_vector_file_safely(&safe, &input, &mut progress).unwrap();
        assert_eq!(actual.status, 0);
        assert!(actual.error.is_none());
        assert_eq!(std::fs::read(&safe).unwrap(), std::fs::read(&legacy).unwrap(), "{format}");
    }
}
