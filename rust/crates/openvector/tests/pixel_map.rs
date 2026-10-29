mod common;
use openvector::pixel_map::PixelMap;

fn ints(v: &serde_json::Value) -> Vec<i64> { v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect() }

#[test]
fn flood_fill_matches_all_5574_original_host_cpu_map_mutations() {
    let reference = common::read_json("pixel-map-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5574);
    for f in cases {
        let (w, h) = (f["width"].as_i64().unwrap(), f["height"].as_i64().unwrap());
        let mut map = PixelMap::new(w, h, &ints(&f["segments"])).unwrap();
        map.flood_fill(f["seed"].as_i64().unwrap(), f["replacement"].as_i64().unwrap()).unwrap();
        let expected: Vec<i32> = ints(&f["expected"]).into_iter().map(|x| x as i32).collect();
        assert_eq!((map.width(), map.height(), map.segments()), (w, h, &expected[..]), "fill {w}x{h} at {}", f["seed"]);
    }
}

#[test]
fn map_storage_and_exported_snapshots_belong_to_separate_owners() {
    let mut segments = vec![0i64, 1, 1, 0];
    let mut map = PixelMap::new(2, 2, &segments).unwrap();
    segments[0] = 99;
    let mut snapshot = map.segments().to_vec();
    snapshot[0] = 100;
    map.flood_fill(0, 7).unwrap();
    assert_eq!(map.segments(), &[7, 1, 1, 0]);
}

#[test]
fn invalid_map_or_edit_inputs_fail_before_any_mutation() {
    for (w, h, s) in [(0i64, 1i64, vec![]), (2, 1, vec![1i64]), (1, 1, vec![2147483648i64])] {
        assert!(PixelMap::new(w, h, &s).is_err());
    }
    let mut map = PixelMap::new(2, 1, &[0, 1]).unwrap();
    for (seed, rep) in [(-1i64, 2i64), (2, 2), (0, 0), (0, 2147483648)] {
        assert!(map.flood_fill(seed, rep).is_err());
        assert_eq!(map.segments(), &[0, 1]);
    }
}
