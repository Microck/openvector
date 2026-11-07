//! Export-stage benchmark from committed, already fitted native fixture graphs.
//!
//! Build the identical example at the base and candidate revisions. Dump the
//! base output with `--dump-dir DIR`, then pass that directory to the candidate
//! with `--check-dir DIR` for full byte equality outside the timed region.
//! Parsing, tiling, hashing and disk I/O are excluded from the measurements.
use openvector::contour_fitting::FittedGeometry;
use openvector::decimal_format::fixed_decimal;
use openvector::dxf::DxfCurveMode;
use openvector::emf::EmfDevice;
use openvector::eps::EpsOptions;
use openvector::vector_output::VectorOutputInput;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

struct Args {
    fixture: String,
    tiles: usize,
    samples: usize,
    formats: Vec<String>,
    dump_dir: Option<PathBuf>,
    check_dir: Option<PathBuf>,
}

fn args() -> Args {
    let mut result = Args {
        fixture: "alpha-profile-3".into(), tiles: 8, samples: 7,
        formats: "decimal,svg,pdf,eps,ai,dxf-splines,dxf-dense,emf".split(',').map(str::to_owned).collect(),
        dump_dir: None, check_dir: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--help" {
            println!("benchmark_export [--fixture alpha-profile-3] [--tiles 8] [--samples 7] \\\n    [--formats decimal,svg,pdf,eps,ai,dxf-splines,dxf-dense,emf] \\\n    [--dump-dir DIR] [--check-dir DIR]");
            std::process::exit(0);
        }
        let value = args.next().expect("every option needs a value");
        match flag.as_str() {
            "--fixture" => result.fixture = value,
            "--tiles" => result.tiles = value.parse().expect("integer tiles"),
            "--samples" => result.samples = value.parse().expect("integer samples"),
            "--formats" => result.formats = value.split(',').map(str::to_owned).collect(),
            "--dump-dir" => result.dump_dir = Some(value.into()),
            "--check-dir" => result.check_dir = Some(value.into()),
            _ => panic!("unknown option {flag}"),
        }
    }
    assert!((1..=64).contains(&result.tiles), "tiles must be 1..=64");
    assert!(result.samples > 0, "samples must be positive");
    result
}

fn mosaic(source: &FittedGeometry, width: i64, height: i64, tiles: usize) -> FittedGeometry {
    let mut result = FittedGeometry { cubic_storage_slots: 0, cubics: Vec::new(), nodes: Vec::new(), regions: Vec::new() };
    for row in 0..tiles {
        for column in 0..tiles {
            let (dx, dy) = ((column as i64 * width) as f64, (row as i64 * height) as f64);
            let nodes = result.nodes.len();
            let regions = result.regions.len();
            let cubics = result.cubic_storage_slots;
            for mut node in source.nodes.clone() {
                node.x += dx;
                node.y += dy;
                if node.cubic >= 0 { node.cubic += cubics as i32; }
                result.nodes.push(node);
            }
            for mut cubic in source.cubics.clone() {
                cubic.index += cubics;
                for point in cubic.points.chunks_exact_mut(2) { point[0] += dx; point[1] += dy; }
                result.cubics.push(cubic);
            }
            for mut region in source.regions.clone() {
                if region.parent >= 0 { region.parent += regions as i32; }
                for node in &mut region.contour { *node += nodes; }
                for edge in &mut region.edges {
                    if edge[1] != 0 { edge[0] += cubics as i64; }
                    if edge[2] >= 0 { edge[2] += regions as i64; }
                    edge[3] += nodes as i64;
                    edge[4] += nodes as i64;
                }
                result.regions.push(region);
            }
            result.cubic_storage_slots += source.cubic_storage_slots;
        }
    }
    result
}

fn export(format: &str, input: &VectorOutputInput<'_>, palette: &[f32], coordinates: &[f64]) -> Vec<u8> {
    match format {
        // Formatter attribution uses the actual fitted coordinates, at each
        // precision used by the text exporters; no fitting or parsing is timed.
        "decimal" => {
            let mut result = Vec::new();
            for precision in [2, 3, 16] {
                for &value in coordinates {
                    result.extend_from_slice(fixed_decimal(black_box(value), precision).as_bytes());
                    result.push(b'\n');
                }
            }
            result
        }
        "svg" => openvector::svg::export_svg(input).unwrap().into_bytes(),
        "pdf" => openvector::pdf::export_pdf(input).unwrap(),
        "eps" => openvector::eps::export_eps(input, &EpsOptions {
            compact: false, creation_date: Some("Sat Nov  7 00:00:00 2026".into()),
        }).unwrap().into_bytes(),
        "ai" => openvector::ai::export_ai(input).unwrap().into_bytes(),
        "dxf-splines" => openvector::dxf::export_dxf(input, palette, DxfCurveMode::Splines).unwrap().into_bytes(),
        "dxf-dense" => openvector::dxf::export_dxf(input, palette, DxfCurveMode::DenseLines).unwrap().into_bytes(),
        "emf" => openvector::emf::export_emf(input, &EmfDevice {
            pixels: [1920, 1080], millimeters: [508, 286], micrometers: [508000, 286000],
        }).unwrap(),
        _ => panic!("unknown format {format}"),
    }
}

fn main() {
    let args = args();
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures");
    let fixture: Value = serde_json::from_slice(&std::fs::read(fixture_path.join("pipeline-fitting-reference.json")).unwrap()).unwrap();
    let case = fixture["cases"].as_array().unwrap().iter().find(|case| case["id"] == args.fixture).expect("fixture ID");
    let source = FittedGeometry::from_json(&case["expected"]).unwrap();
    let dimensions: Value = serde_json::from_slice(&std::fs::read(fixture_path.join("pipeline-contour-reference.json")).unwrap()).unwrap();
    let dimensions = dimensions["cases"].as_array().unwrap().iter().find(|case| case["id"] == args.fixture).expect("fixture dimensions");
    let (width, height) = (dimensions["width"].as_i64().unwrap(), dimensions["height"].as_i64().unwrap());
    let geometry = mosaic(&source, width, height, args.tiles);
    let input = VectorOutputInput::new(&geometry, width * args.tiles as i64, height * args.tiles as i64);
    let color_count = source.regions.iter().map(|region| region.color_index).max().unwrap() as usize + 1;
    let mut palette = vec![0.0; color_count * 4];
    for region in &source.regions {
        for channel in 0..4 { palette[region.color_index as usize * 4 + channel] = region.bgra[channel] as f32 / 255.0; }
    }
    let coordinates: Vec<f64> = geometry.nodes.iter().flat_map(|node| [node.x, node.y])
        .chain(geometry.cubics.iter().flat_map(|cubic| cubic.points)).collect();
    if let Some(path) = &args.dump_dir { std::fs::create_dir_all(path).unwrap(); }
    for format in &args.formats {
        // One untimed warmup initializes both lazy tables and allocator paths.
        let reference = export(format, &input, &palette, &coordinates);
        let filename = format!("{}-{}x{}-{format}.bin", args.fixture, args.tiles, args.tiles);
        if let Some(path) = &args.dump_dir { std::fs::write(path.join(&filename), &reference).unwrap(); }
        if let Some(path) = &args.check_dir {
            assert_eq!(reference, std::fs::read(path.join(&filename)).unwrap(), "baseline bytes changed: {filename}");
        }
        let mut samples = Vec::new();
        for _ in 0..args.samples {
            let start = Instant::now();
            let bytes = black_box(export(black_box(format), black_box(&input), black_box(&palette), black_box(&coordinates)));
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(bytes, reference, "nondeterministic export: {filename}");
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let median = if sorted.len() % 2 == 0 { (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0 }
            else { sorted[sorted.len() / 2] };
        println!("{}", json!({"fixture": args.fixture, "tiles": args.tiles, "format": format,
            "nodes": geometry.nodes.len(), "regions": geometry.regions.len(), "cubics": geometry.cubics.len(),
            "coordinates": coordinates.len(), "bytes": reference.len(), "sha256": format!("{:x}", Sha256::digest(&reference)),
            "median_ms": median, "min_ms": sorted[0], "max_ms": sorted[sorted.len()-1], "samples_ms": samples}));
    }
}
