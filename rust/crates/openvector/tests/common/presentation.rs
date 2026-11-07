//! Application-only expectations derived from archived colors, never engine output.
use std::collections::{BTreeMap, BTreeSet};

fn straight_rgb(rgb: &str, alpha: u32) -> String {
    assert_eq!(rgb.len(), 6);
    (0..6).step_by(2).map(|at| {
        let component = u32::from_str_radix(&rgb[at..at + 2], 16).unwrap();
        let straight = if alpha == 0 { 0 } else { ((component * 255 + alpha / 2) / alpha).min(255) };
        format!("{straight:02x}")
    }).collect()
}

/// Captured grouped SVG carries the exact alpha byte in each group ID. Project
/// only its ID and paint RGB; preserve every path, opacity, and surrounding byte.
/// Do not apply this to low-level serializers, whose BGRA input is literal.
pub fn application_svg_fixture(bytes: &[u8]) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).unwrap();
    let mut alphas = Vec::new();
    let mut output = String::with_capacity(text.len());
    for part in text.split_inclusive('>') {
        let Some(start) = part.rfind('<') else { output.push_str(part); continue };
        let (prefix, tag) = part.split_at(start);
        output.push_str(prefix);
        if tag.starts_with("<g id=\"#") {
            let color = tag.strip_prefix("<g id=\"#").unwrap().strip_suffix("\">").unwrap();
            assert_eq!(color.len(), 8);
            let alpha = u32::from_str_radix(&color[6..], 16).unwrap();
            alphas.push(alpha);
            output.push_str(&format!("<g id=\"#{}{}\">", straight_rgb(&color[..6], alpha), &color[6..]));
        } else if tag == "</g>" {
            alphas.pop().expect("balanced captured SVG groups");
            output.push_str(tag);
        } else {
            let mut projected = tag.to_owned();
            if let Some(&alpha) = alphas.last() {
                for attribute in ["fill=\"#", "stroke=\"#"] {
                    if let Some(at) = projected.find(attribute) {
                        let at = at + attribute.len();
                        assert_eq!(&projected[at + 6..at + 7], "\"");
                        let rgb = straight_rgb(&projected[at..at + 6], alpha);
                        projected.replace_range(at..at + 6, &rgb);
                    }
                }
            }
            output.push_str(&projected);
        }
    }
    assert!(alphas.is_empty(), "balanced captured SVG groups");
    output.into_bytes()
}

type Field = (u16, String);
type Record = Vec<Field>;

fn dxf_reference(code: u16) -> bool {
    (330..=369).contains(&code) || (390..=399).contains(&code) || (480..=481).contains(&code) || code == 1005
}

fn dxf_drawing(bytes: &[u8]) -> (Vec<Record>, BTreeMap<String, Record>, Vec<Record>) {
    let text = std::str::from_utf8(bytes).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len() % 2, 0, "complete DXF fields");
    let fields = lines.chunks_exact(2).map(|pair| (pair[0].trim().parse::<u16>().unwrap(), pair[1].to_owned())).collect::<Vec<_>>();
    assert_eq!(fields.last(), Some(&(0, "EOF".into())), "complete DXF file");
    let mut handles = BTreeSet::new();
    let mut references = Vec::new();
    let mut section = "";
    for (index, (code, value)) in fields.iter().enumerate() {
        if *code == 0 && value == "SECTION" {
            assert!(section.is_empty(), "nested DXF section");
            assert_eq!(fields[index + 1].0, 2);
            section = &fields[index + 1].1;
        } else if *code == 0 && value == "ENDSEC" {
            assert!(!section.is_empty(), "unmatched DXF section end");
            section = "";
        }
        if (*code == 5 || *code == 105) && section != "HEADER" {
            let handle = u64::from_str_radix(value, 16).expect("hexadecimal DXF handle");
            assert_ne!(handle, 0, "non-null DXF object handle");
            assert!(handles.insert(handle), "unique DXF object handles: {value}");
        }
        if dxf_reference(*code) {
            let handle = u64::from_str_radix(value, 16).expect("hexadecimal DXF reference");
            if handle != 0 { references.push(handle); }
        }
    }
    assert!(section.is_empty(), "closed DXF sections");
    for handle in references { assert!(handles.contains(&handle), "DXF reference resolves: {handle:X}"); }
    let mut records: Vec<Record> = Vec::new();
    for field in fields {
        if field.0 == 0 || records.is_empty() { records.push(Vec::new()); }
        records.last_mut().unwrap().push(field);
    }
    let mut entities = Vec::new();
    let mut layers = BTreeMap::new();
    let mut metadata = Vec::new();
    let mut layer_count = None;
    let mut section = String::new();
    for mut record in records {
        let kind = record[0].1.as_str();
        if kind == "SECTION" {
            section = record.iter().find(|field| field.0 == 2).unwrap().1.clone();
            if section == "HEADER" {
                // The next allocated handle varies with omitted palette entries.
                // Preserve all other header variables, including drawing units.
                let mut seed = false;
                record.retain(|field| {
                    if field.0 == 9 { seed = field.1 == "$HANDSEED"; }
                    !seed
                });
            }
            metadata.push(record);
        } else if kind == "ENDSEC" {
            section.clear();
            metadata.push(record);
        } else if section == "ENTITIES" {
            entities.push(record);
        } else if section == "TABLES" && kind == "LAYER" {
            let name = record.iter().find(|field| field.0 == 2).unwrap().1.clone();
            assert!(layers.insert(name, record).is_none(), "unique DXF layers");
        } else {
            if section == "TABLES" && kind == "TABLE" && record.iter().any(|field| field.0 == 2 && field.1 == "LAYER") {
                assert!(layer_count.is_none(), "one DXF layer table");
                layer_count = Some(record.iter().find(|field| field.0 == 70).unwrap().1.parse::<usize>().unwrap());
                record.retain(|field| field.0 != 70);
            }
            metadata.push(record);
        }
    }
    assert_eq!(layer_count, Some(layers.len()), "DXF layer count matches declarations");
    // Objects after ENTITIES also receive shifted handles when a layer is
    // omitted. Compare object identity and reference topology, not ID numbers.
    let mut identities = BTreeMap::new();
    let mut register = |record: &Record, identity: String| {
        for (index, (code, value)) in record.iter().enumerate() {
            if *code == 5 || *code == 105 {
                let handle = u64::from_str_radix(value, 16).unwrap();
                assert!(identities.insert(handle, format!("{identity}:{index}")).is_none());
            }
        }
    };
    for (index, record) in entities.iter().enumerate() { register(record, format!("entity:{index}")); }
    for (name, record) in &layers { register(record, format!("layer:{name}")); }
    for (index, record) in metadata.iter().enumerate() {
        let header = record[0].0 == 0 && record[0].1 == "SECTION" && record.iter().any(|field| field.0 == 2 && field.1 == "HEADER");
        if !header { register(record, format!("metadata:{index}")); }
    }
    let normalize = |record: &mut Record| {
        for (code, value) in record {
            if *code == 5 || *code == 105 || dxf_reference(*code) {
                let handle = u64::from_str_radix(value, 16).unwrap();
                *value = if handle == 0 { "0".into() } else { identities.get(&handle).expect("registered DXF object").clone() };
            }
        }
    };
    for record in entities.iter_mut().chain(layers.values_mut()).chain(metadata.iter_mut()) { normalize(record); }
    (entities, layers, metadata)
}

/// Ignore only unused palette declarations and allocation-dependent handles.
/// Every drawing field and every field of each used layer remains exact.
pub fn assert_dxf_drawing(actual: &[u8], expected: &[u8], context: &str) {
    let (observed, layers, metadata) = dxf_drawing(actual);
    let (reference, expected_layers, expected_metadata) = dxf_drawing(expected);
    assert_eq!(metadata, expected_metadata, "{context}: complete DXF drawing metadata");
    assert_eq!(observed, reference, "{context}: complete DXF drawing entities");
    let used = reference.iter().flatten().filter(|field| field.0 == 8).map(|field| &field.1).collect::<BTreeSet<_>>();
    for name in used {
        let expected = expected_layers.get(name).expect("captured drawing declares used layer");
        assert_eq!(layers.get(name), Some(expected), "{context}: used DXF layer {name}");
    }
}

#[test]
fn svg_projection_preserves_paths_alpha_and_structure() {
    let raw = b"<svg><g id=\"#f4c328fe\"><path fill=\"#f4c328\" stroke=\"#f4c328\" opacity=\"1.00\" d=\" M 1.23 4.56 Z\" /></g><g id=\"#ff804000\"><path fill=\"#ff8040\" opacity=\"0.00\" /></g></svg>\n";
    let expected = b"<svg><g id=\"#f5c428fe\"><path fill=\"#f5c428\" stroke=\"#f5c428\" opacity=\"1.00\" d=\" M 1.23 4.56 Z\" /></g><g id=\"#00000000\"><path fill=\"#000000\" opacity=\"0.00\" /></g></svg>\n";
    assert_eq!(application_svg_fixture(raw), expected);
    assert_eq!(straight_rgb("804020", 128), "ff8040");
    assert_eq!(straight_rgb("ffffff", 1), "ffffff");
    assert_eq!(straight_rgb("123456", 255), "123456");
}

#[test]
fn dxf_application_comparison_retains_entities_layers_and_handle_integrity() {
    let expected = b"999\nOpenVector fixture\n0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n9\n$EXTMIN\n10\n0.0\n9\n$HANDSEED\n5\n10\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n2\n0\nLAYER\n5\n1\n2\npaint\n62\n7\n0\nLAYER\n5\n2\n2\nunused\n62\n7\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n3\n8\npaint\n330\n1\n10\n1.25\n20\n2.5\n0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nF\n281\n1\n0\nENDSEC\n0\nEOF\n";
    // Unused entries change the layer count, handle allocation and next handle.
    let actual = b"999\nOpenVector fixture\n0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n9\n$EXTMIN\n10\n0.0\n9\n$HANDSEED\n5\n11\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n1\n0\nLAYER\n5\n1\n2\npaint\n62\n7\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n2\n8\npaint\n330\n1\n10\n1.25\n20\n2.5\n0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nF\n281\n1\n0\nENDSEC\n0\nEOF\n";
    assert_dxf_drawing(actual, expected, "unused layer");
    let text = std::str::from_utf8(actual).unwrap();
    for (before, after) in [
        ("10\n1.25", "10\n1.26"),
        ("62\n7", "62\n8"),
        ("5\n2", "5\n1"),
        ("330\n1", "330\nE"),
        ("330\n1", "330\nF"),
        ("8\npaint", "8\nmissing"),
        ("OpenVector fixture", "Different producer"),
        ("$INSUNITS\n70\n1", "$INSUNITS\n70\n4"),
        ("$EXTMIN\n10\n0.0", "$EXTMIN\n10\n1.0"),
        ("281\n1", "281\n0"),
        ("2\nLAYER\n70\n1", "2\nLAYER\n70\n2"),
    ] {
        let damaged = text.replace(before, after);
        assert!(std::panic::catch_unwind(|| assert_dxf_drawing(damaged.as_bytes(), expected, "damaged drawing")).is_err(), "{before}");
    }
    let missing_entity = text.replace("0\nLINE\n5\n2\n8\npaint\n330\n1\n10\n1.25\n20\n2.5\n", "");
    assert!(std::panic::catch_unwind(|| assert_dxf_drawing(missing_entity.as_bytes(), expected, "missing entity")).is_err());
}
