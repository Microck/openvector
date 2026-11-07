//! Stage-only fitting benchmark. See docs/performance/fitting.md for reproduction.
use std::{hint::black_box, io::Read, path::PathBuf, time::Instant};
use openvector::{contour_fitting::{fit_contours, ContourFitter, FittedGeometry, FitterSnapshot, FittingParameters, FittingProgress}, contours::{Geometry, Node, Region}, fitting::FitNode};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

struct Case { id: String, geometry: Geometry, parameters: FittingParameters, expected: Option<FittedGeometry> }
fn fixtures() -> PathBuf { std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures")) }
fn json_file(name: &str) -> Value { serde_json::from_slice(&std::fs::read(fixtures().join(name)).unwrap()).unwrap() }
fn geometry(v: &Value) -> Geometry { serde_json::from_value(v.clone()).unwrap() }
fn pipeline_cases() -> Vec<Case> {
    json_file("pipeline-fitting-reference.json")["cases"].as_array().unwrap().iter().map(|fx| Case {
        id: format!("pipeline/{}", fx["id"].as_str().unwrap()), geometry: geometry(&fx["geometry"]),
        parameters: FittingParameters::from_json(&fx["parameters"]), expected: Some(FittedGeometry::from_json(&fx["expected"]).unwrap()),
    }).collect()
}
fn recorded_cases(name: &str) -> Vec<Case> {
    let raw = std::fs::read(fixtures().join(name)).unwrap();
    let mut decoded = String::new(); flate2::read::MultiGzDecoder::new(&raw[..]).read_to_string(&mut decoded).unwrap();
    let rows: Vec<Value> = decoded.lines().map(|s| serde_json::from_str(s).unwrap()).collect();
    let registry = rows[0]["parameters"].as_object().unwrap();
    rows.iter().skip(1).map(|row| {
        let state = &row["expectedAutomatic"]["state"];
        let parameter = |key: &str| {
            let descriptor = &registry[key]; assert_eq!(descriptor["count"].as_u64(), Some(1));
            let text = state["parameterBits"][key].as_str().unwrap(); assert_eq!(text.len() % 2, 0);
            let bits: Vec<u8> = text.as_bytes().chunks_exact(2).map(|s| u8::from_str_radix(std::str::from_utf8(s).unwrap(), 16).unwrap()).collect();
            match descriptor["kind"].as_str().unwrap() {
                "int32" => i32::from_le_bytes(bits.try_into().unwrap()) as f64,
                "float32" => f32::from_le_bytes(bits.try_into().unwrap()) as f64,
                "float64" => f64::from_le_bytes(bits.try_into().unwrap()),
                kind => panic!("unexpected fitting parameter kind {kind}"),
            }
        };
        Case { id: row["id"].as_str().unwrap().to_owned(), geometry: geometry(&state["contours"]),
            parameters: FittingParameters { stat_thresh_initial: parameter("BezierFitter::stat_thresh_initial"), stat_thresh_final: parameter("BezierFitter::stat_thresh_final"), max_iterations: parameter("BezierFitter::max_iterations") },
            // Captured contours already have post-fitting kinds. ContourFitter::run
            // normalizes those kinds before fitting, just as a repeated live run does.
            expected: Some(FittedGeometry::from_json(&state["fittedGeometry"]).unwrap()) }
    }).collect()
}
fn authored_case(regions: usize, samples: usize, interval: usize) -> Case {
    let mut geometry = Geometry { nodes: Vec::new(), regions: Vec::new() };
    for r in 0..regions {
        let start = geometry.nodes.len();
        for p in 0..samples {
            // Polygonal waves with exact binary coordinates; no transcendental setup.
            let quarter = samples / 4; let pos = p % quarter; let side = p / quarter;
            let (x, y) = match side { 0 => (pos, 0), 1 => (quarter, pos), 2 => (quarter-pos, quarter), _ => (0, quarter-pos) };
            let dx = (p * 13 % 7) as f64 * 0.125; let dy = (p * 17 % 11) as f64 * 0.0625;
            geometry.nodes.push(Node { x: x as f64 + r as f64 * 100.0 + dx, y: y as f64 + dy,
                kind: if p % interval == 0 { 3 } else { 0 }, flags: 0 });
        }
        geometry.regions.push(Region { pixel_count: 100, color_index: 0, bgra: [20,40,60,255], parent: -1,
            contour: (start..start+samples).collect(), boundary: vec![[0,-1,1]; samples], neighbors: vec![-1] });
    }
    Case { id: format!("authored/{regions}x{samples}-interval-{interval}"), geometry,
        parameters: FittingParameters { stat_thresh_initial: 0.01, stat_thresh_final: 0.1, max_iterations: 10.0 }, expected: None }
}
fn u(h: &mut Sha256, n: usize) { h.update((n as u64).to_le_bytes()); }
fn i(h: &mut Sha256, n: i64) { h.update(n.to_le_bytes()); }
fn f(h: &mut Sha256, n: f64) { h.update(n.to_bits().to_le_bytes()); }
fn nodes(h: &mut Sha256, nodes: &[FitNode]) {
    u(h,nodes.len()); for n in nodes { f(h,n.x); f(h,n.y); h.update([n.kind]); i(h,n.cubic as i64); }
}
fn fitted_hash(g: &FittedGeometry) -> String {
    let mut h=Sha256::new(); u(&mut h,g.cubic_storage_slots); u(&mut h,g.cubics.len());
    for c in &g.cubics { u(&mut h,c.index); for &v in &c.points { f(&mut h,v); } }
    nodes(&mut h,&g.nodes); u(&mut h,g.regions.len());
    for r in &g.regions { h.update(r.bgra); i(&mut h,r.pixel_count as i64); i(&mut h,r.color_index as i64); i(&mut h,r.parent as i64);
        u(&mut h,r.contour.len()); for &n in &r.contour { u(&mut h,n); }
        u(&mut h,r.edges.len()); for e in &r.edges { for &n in e { i(&mut h,n); } }
    }
    format!("{:x}",h.finalize())
}
fn snapshot_hash(s: &FitterSnapshot) -> String {
    let g=FittedGeometry { cubic_storage_slots:s.cubic_storage_slots,cubics:s.cubics.clone(),nodes:s.nodes.clone(),regions:s.regions.clone() };
    let mut h=Sha256::new(); h.update(fitted_hash(&g)); u(&mut h,s.cubic_availability.len());h.update(&s.cubic_availability);
    for r in &s.regions { u(&mut h,r.boundary.len());for b in &r.boundary {for &n in b {i(&mut h,n as i64);}} }
    format!("{:x}",h.finalize())
}
fn input_hash(c: &Case) -> String {
    let mut h=Sha256::new();u(&mut h,c.geometry.nodes.len()); for n in &c.geometry.nodes {f(&mut h,n.x);f(&mut h,n.y);h.update([n.kind,n.flags]);}
    h.update(serde_json::to_vec(&c.geometry.regions).unwrap()); f(&mut h,c.parameters.stat_thresh_initial); f(&mut h,c.parameters.stat_thresh_final);f(&mut h,c.parameters.max_iterations);
    format!("{:x}",h.finalize())
}
fn verify(c: &Case, expected: &str) -> Value {
    let mut fitter=ContourFitter::new(&c.geometry).unwrap(); let mut h=Sha256::new(); let mut event_count=0usize;
    let mut observer=|e:&FittingProgress, live:&ContourFitter| {
        i(&mut h,e.phase as i64);i(&mut h,e.substep as i64);i(&mut h,e.counter as i64);f(&mut h,e.fraction);i(&mut h,e.percent as i64);
        h.update(snapshot_hash(&live.snapshot()));event_count+=1;Ok(())
    };
    assert_eq!(fitted_hash(&fitter.run(&c.parameters,Some(&mut observer)).unwrap()),expected);
    let progress=format!("{:x}",h.finalize());
    let stops:Vec<_>=[0,event_count/2,event_count-1].into_iter().collect(); let mut cancelled=Vec::new();
    for stop in stops {
        let mut fitter=ContourFitter::new(&c.geometry).unwrap();let mut n=0usize;let mut observed=String::new();
        let mut cb=|_:&FittingProgress,live:&ContourFitter| {let current=n;n+=1;if current==stop {observed=snapshot_hash(&live.snapshot());Err("benchmark-cancel".to_owned())} else {Ok(())}};
        assert_eq!(fitter.run(&c.parameters,Some(&mut cb)).unwrap_err(),"benchmark-cancel");
        let retained=snapshot_hash(&fitter.snapshot());assert_eq!(retained,observed); cancelled.push(json!({"event_index":stop,"retained_snapshot":retained}));
        // Retry the cancelled live instance; include both result and retained storage.
        let resumed=fitter.run(&c.parameters,None).unwrap();cancelled.last_mut().unwrap()["resumed_result"]=json!(fitted_hash(&resumed));
        cancelled.last_mut().unwrap()["resumed_snapshot"]=json!(snapshot_hash(&fitter.snapshot()));
    }
    json!({"progress_events":event_count,"progress_sha256":progress,"cancellation":cancelled})
}
fn main() {
    let args:Vec<_>=std::env::args().collect(); let arg=|flag:&str|args.windows(2).find(|a|a[0]==flag).map(|a|a[1].as_str());
    let samples:usize=arg("--samples").unwrap_or("3").parse().unwrap();let iterations:usize=arg("--iterations").unwrap_or("1").parse().unwrap();
    assert!(samples>0 && iterations>0);let check=args.iter().any(|a|a=="--verify");let filter=arg("--case");
    let mut cases=pipeline_cases();cases.extend(recorded_cases("fresh-representative-20261020.ndjson.gz"));cases.extend(recorded_cases("fresh-photographic-20261020.ndjson.gz"));
    cases.push(authored_case(128,16,4));cases.push(authored_case(32,128,128));
    let mut results=Vec::new();
    for c in cases { if filter.is_some_and(|s|!c.id.contains(s)) {continue;}
        let actual=fit_contours(&c.geometry,&c.parameters,None).unwrap();let digest=fitted_hash(&actual);
        let oracle=c.expected.as_ref().map(|g|fitted_hash(g));
        if let Some(expected)=&oracle {assert_eq!(&digest,expected,"recorded fitting result {}",c.id);}
        let checks=if check {verify(&c,&digest)} else {Value::Null};
        // One warmup, separate reference validation, and all hashing are outside timers.
        black_box(fit_contours(black_box(&c.geometry),black_box(&c.parameters),None).unwrap());
        let mut times=Vec::new();for _ in 0..samples {let begin=Instant::now();for _ in 0..iterations {black_box(fit_contours(black_box(&c.geometry),black_box(&c.parameters),None).unwrap());}times.push(begin.elapsed().as_secs_f64()*1000.0/iterations as f64);}
        results.push(json!({"id":c.id,"nodes":c.geometry.nodes.len(),"regions":c.geometry.regions.len(),"contour_samples":c.geometry.regions.iter().map(|r|r.contour.len()).sum::<usize>(),"cubic_slots":actual.cubic_storage_slots,"input_sha256":input_hash(&c),"output_sha256":digest,"native_oracle_sha256":oracle,"checks":checks,"iterations":iterations,"samples_ms":times}));
    }
    println!("{}",json!({"cases":results}));
}
