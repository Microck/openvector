mod common;
mod smoothing_support;
use openvector::smoothing::*;
use serde_json::{json, Value};
use smoothing_support::*;
use std::collections::BTreeSet;

fn prior_input(fx: &Value) -> (Vec<Vec<f64>>, Vec<f64>) {
    (fx["points"].as_array().unwrap().iter().map(f64s).collect(), f64s(&fx["sampleCounts"]))
}
fn bits(a: f64, b: &Value, m: &str) { assert_eq!(a.to_bits(), f(b).to_bits(), "{m}: {a} vs {b}"); }
fn pair(a: [f64; 2], b: &Value, m: &str) { assert_same(&json!(a.to_vec()), b, m); }

#[test]
fn quadratic_priors_match_all_5016_original_calculations() {
    let reference = common::read_json("curvature-prior-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5016);
    for (index, fx) in cases.iter().enumerate() {
        let (p, c) = prior_input(fx);
        let a = quadratic_prior(&p, &c).unwrap(); let e = &fx["quadraticExpected"];
        bits(a.energy, &e["energy"], &format!("case {index} energy"));
        pair(a.first_gradient, &e["firstGradient"], &format!("case {index} first derivative"));
        pair(a.last_gradient, &e["lastGradient"], &format!("case {index} last derivative"));
    }
}

#[test]
fn curvature_priors_match_all_5010_original_calculations() {
    let reference = common::read_json("curvature-prior-reference.json");
    let finite: Vec<_> = reference["cases"].as_array().unwrap().iter().filter(|c| !c["degenerate"].as_bool().unwrap_or(false)).collect();
    assert_eq!(finite.len(), 5010);
    for (index, fx) in finite.iter().enumerate() {
        let (p, c) = prior_input(fx);
        let a = curvature_prior(&p, &c, f(&fx["lengthWeight"])).unwrap(); let e = &fx["expected"];
        bits(a.energy, &e["energy"], &format!("case {index} energy"));
        pair(a.first_gradient, &e["firstGradient"], &format!("case {index} first derivative"));
        pair(a.last_gradient, &e["lastGradient"], &format!("case {index} last derivative"));
    }
}

#[test]
fn collapsed_edges_retain_native_nan_results() {
    let reference = common::read_json("curvature-prior-reference.json");
    let degenerate: Vec<_> = reference["cases"].as_array().unwrap().iter().filter(|c| c["degenerate"].as_bool().unwrap_or(false)).collect();
    assert_eq!(degenerate.len(), 6);
    for fx in degenerate {
        assert_eq!(fx["expected"], json!({"energy": "NaN", "firstGradient": ["NaN", "NaN"], "lastGradient": ["NaN", "NaN"]}));
        let (p, c) = prior_input(fx);
        let a = curvature_prior(&p, &c, f(&fx["lengthWeight"])).unwrap();
        assert!(a.energy.is_nan());
        assert!(a.first_gradient.iter().chain(&a.last_gradient).all(|v| v.is_nan()));
    }
}

#[test]
fn prior_derivatives_agree_with_finite_differences() {
    let points = vec![vec![0.25, 0.5], vec![1.5, 0.7], vec![2.1, 2.4]]; let counts = vec![1.0, 3.0]; let step = 1e-6;
    let priors: [(&str, &dyn Fn(&[Vec<f64>]) -> PriorTerms); 2] = [
        ("curvaturePrior", &|p| curvature_prior(p, &counts, 0.5).unwrap()),
        ("quadraticPrior", &|p| quadratic_prior(p, &counts).unwrap())];
    for (name, prior) in priors {
        let t = prior(&points);
        for (point, gradient) in [(0, t.first_gradient), (2, t.last_gradient)] {
            for axis in 0..2 {
                let mut before = points.clone(); let mut after = points.clone();
                before[point][axis] -= step; after[point][axis] += step;
                let d = (prior(&after).energy - prior(&before).energy) / (2.0 * step);
                assert!((d - gradient[axis]).abs() < 1e-8, "{name}, point {point}, axis {axis}");
            }
        }
    }
}

#[test]
fn curvature_prior_validates_its_public_boundary() {
    let p = vec![vec![0.0, 0.0], vec![1.0, 1.0], vec![3.0, 1.0]]; let c = vec![1.0, 2.0];
    assert!(curvature_prior(&p, &c, 0.1).is_ok() && quadratic_prior(&p, &c).is_ok());
    let bad_points: Vec<Vec<Vec<f64>>> = vec![vec![], vec![vec![], vec![], vec![]], vec![vec![0.0, 0.0], vec![f64::NAN, 1.0], vec![2.0, 2.0]]];
    for bp in &bad_points { assert!(curvature_prior(bp, &c, 0.1).is_err()); }
    for bc in [vec![0.0, 1.0], vec![1.0, 2147483648.0], vec![1.0, 1.5], vec![f64::NAN, f64::NAN]] { assert!(curvature_prior(&p, &bc, 0.1).is_err()); }
    for w in [-1.0, f64::INFINITY] { assert!(curvature_prior(&p, &c, w).is_err()); }
}

#[test]
fn prepared_objective_matches_all_5598_native_states() {
    let reference = common::read_json("smoothing-objective-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5598);
    let (mut phases, mut measurements, mut priors) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let (mut flagged, mut penalized, mut small, mut smoothed) = (0, 0, 0, 0);
    for (index, fx) in cases.iter().enumerate() {
        let o = Owned::new(fx);
        let a = smoothing_objective(&mut o.input(fx)).unwrap();
        bits(a.energy, &fx["expected"]["energy"], &format!("case {index} energy"));
        assert_same(&json!(a.gradient), &fx["expected"]["gradient"], &format!("case {index} gradient"));
        let phase = i(&fx["phase"]) as usize;
        phases.insert(phase);
        measurements.insert(i(&fx["parameters"]["ContourSmoother::measurement_types"][phase]));
        priors.insert(i(&fx["parameters"]["ContourSmoother::prior_types"][phase]));
        flagged += o.geometry.nodes.iter().filter(|n| n.flags & 16 != 0).count();
        penalized += o.penalties.iter().filter(|&&c| c > 0).count();
        small += o.geometry.regions.iter().filter(|r| r.pixel_count < 8).count();
        if fx.get("sourcePipeline").is_some_and(|v| !v.is_null() && v != &json!(false) && v != &json!(0) && v != &json!("")) { smoothed += 1; }
    }
    assert_eq!(phases.into_iter().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(measurements.into_iter().collect::<Vec<_>>(), vec![0, 2]);
    assert_eq!(priors.into_iter().collect::<Vec<_>>(), vec![0, 3]);
    assert!(flagged > 0 && penalized > 0 && small > 0);
    assert_eq!(smoothed, 90);
}

#[test]
fn prepared_derivatives_agree_with_finite_differences_on_free_coordinates() {
    let reference = common::read_json("smoothing-objective-reference.json");
    let fx = &reference["cases"][1];
    let o = Owned::new(fx);
    let gradient = smoothing_objective(&mut o.input(fx)).unwrap().gradient;
    let b: Vec<usize> = o.borders.iter().map(|&v| v as usize).collect();
    let (left, right, top, bottom) = (b[0], b[1], b[2], b[3]);
    for node in 0..o.geometry.nodes.len() {
        for axis in 0..2 {
            let corner = [0, left - 1, left, left + right - 1].contains(&node);
            let constrained = corner || (axis == 0 && node < left + right) || (axis == 1 && node >= left + right && node < left + right + top + bottom);
            if constrained { assert_eq!(gradient[node * 2 + axis], 0.0); continue; }
            let step = 1e-6;
            let energy = |delta: f64| {
                let mut c = Owned::new(fx);
                if axis == 0 { c.geometry.nodes[node].x += delta } else { c.geometry.nodes[node].y += delta }
                smoothing_objective(&mut c.input(fx)).unwrap().energy
            };
            let d = (energy(step) - energy(-step)) / (2.0 * step);
            assert!((d - gradient[node * 2 + axis]).abs() < 1e-6, "node {node} axis {axis}");
        }
    }
}

fn with_param(fx: &Value, key: &str, value: Value) -> Value {
    let mut c = fx.clone(); c["parameters"][key] = value; c
}

#[test]
fn prepared_objective_validates_its_boundary() {
    let reference = common::read_json("smoothing-objective-reference.json");
    let fx = &reference["cases"][1];
    let mut invalid = vec![];
    let mut c = fx.clone(); c["phase"] = json!(3); invalid.push(c);
    let mut c = fx.clone(); c["geometry"] = json!({"nodes": [], "regions": []}); invalid.push(c);
    let mut c = fx.clone(); c["referencePositions"] = json!([]); invalid.push(c);
    let mut c = fx.clone(); c["penaltyCounts"] = json!([]); invalid.push(c);
    let mut c = fx.clone(); c["borderCounts"] = json!([1, 1, 0, 0]); invalid.push(c);
    invalid.push(with_param(fx, "ContourSmoother::measurement_types", json!([1, 1, 1])));
    invalid.push(with_param(fx, "ContourSmoother::prior_types", json!([1, 1, 1])));
    invalid.push(with_param(fx, "ContourSmoother::air_pressure_weight", json!(-1)));
    for c in &invalid { let o = Owned::new(c); assert!(smoothing_objective(&mut o.input(c)).is_err()); }
    let mut o = Owned::new(fx); o.references.iter_mut().for_each(|v| *v = f64::INFINITY);
    assert!(smoothing_objective(&mut o.input(fx)).is_err());
    let o = Owned::new(fx);
    assert_eq!(smoothing_objective(&mut o.input(fx)).unwrap(), smoothing_objective(&mut o.input(fx)).unwrap());
}

#[test]
fn quadratic_line_search_matches_all_3312_original_results() {
    let reference = common::read_json("smoothing-line-search-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3312);
    let (mut variants, mut iterations) = (BTreeSet::new(), BTreeSet::new());
    let (mut zero, mut negative, mut maximum) = (0, 0, 0);
    for (index, fx) in cases.iter().enumerate() {
        let o = Owned::new(fx);
        let a = smoothing_line_search(&mut o.input(fx)).unwrap();
        assert_same(&a.to_json(), &fx["expected"], &format!("line case {index}"));
        variants.insert(i(&fx["lineVariant"])); iterations.insert(a.iterations);
        if a.step == 0.0 { zero += 1; }
        if a.step < 0.0 { negative += 1; }
        if a.epsilon == f(&fx["parameters"][format!("ContourSmoother::phase_{}.cg_max_eps", i(&fx["phase"]))]) { maximum += 1; }
    }
    assert_eq!(variants.into_iter().collect::<Vec<_>>(), (0..8).collect::<Vec<_>>());
    assert_eq!(iterations.into_iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    assert_eq!((zero, negative, maximum), (860, 143, 751));
}

#[test]
fn line_search_validates_the_prepared_cache_and_phase_controls() {
    let reference = common::read_json("smoothing-line-search-reference.json");
    let fx = &reference["cases"][8];
    let o = Owned::new(fx);
    assert_eq!(smoothing_line_search(&mut o.input(fx)).unwrap(), smoothing_line_search(&mut o.input(fx)).unwrap());
    let prefix = format!("ContourSmoother::phase_{}.", i(&fx["phase"]));
    let mut invalid = vec![];
    let mut c = fx.clone(); c["width"] = json!(1); invalid.push(c);
    let mut c = fx.clone(); c["epsilon"] = json!(0); invalid.push(c);
    let mut c = fx.clone(); c["direction"] = json!([]); invalid.push(c);
    let mut c = fx.clone(); c["nearBorderNodes"] = json!([0]); invalid.push(c);
    let mut c = fx.clone(); c["nearBorderNodes"] = json!([o.geometry.nodes.len()]); invalid.push(c);
    for (k, v) in [("cg_min_eps", json!(0)), ("cg_max_eps", json!(0)), ("cg_max_step_size", json!(-1)),
        ("cg_quad_line_srch_max_iter", json!(0)), ("cg_use_eps_from_step", json!(2))] {
        invalid.push(with_param(fx, &format!("{prefix}{k}"), v));
    }
    for c in &invalid { let o = Owned::new(c); assert!(smoothing_line_search(&mut o.input(c)).is_err()); }
    let mut o = Owned::new(fx); o.direction.iter_mut().for_each(|v| *v = f64::NAN);
    assert!(smoothing_line_search(&mut o.input(fx)).is_err());
    let o = Owned::new(fx); let mut s = o.input(fx); s.height = f64::INFINITY;
    assert!(smoothing_line_search(&mut s).is_err());
}

#[test]
fn conjugate_gradient_matches_all_588_native_controllers() {
    let reference = common::read_json("smoothing-optimizer-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 588);
    let (mut variants, mut phases) = (BTreeSet::new(), BTreeSet::new());
    let (mut greatest, mut early, mut restarts) = (0, 0, 0);
    for (index, fx) in cases.iter().enumerate() {
        let o = Owned::new(fx);
        let a = smoothing_optimize(&mut o.input(fx), None).unwrap();
        assert_same(&a.to_json(), &fx["expected"], &format!("optimizer case {index}"));
        variants.insert(i(&fx["optimizerVariant"])); phases.insert(i(&fx["phase"]));
        greatest = greatest.max(a.iterations); restarts += a.restarts;
        if (a.iterations as f64) < f(&fx["parameters"][format!("ContourSmoother::phase_{}.cg_max_iter", i(&fx["phase"]))]) { early += 1; }
    }
    assert_eq!(variants.into_iter().collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    assert_eq!(phases.into_iter().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(greatest, 260);
    assert!(early > 0 && restarts > 0);
}

#[test]
fn optimizer_rejects_unsupported_controllers_or_invalid_stopping_controls() {
    let reference = common::read_json("smoothing-optimizer-reference.json");
    let fx = &reference["cases"][7];
    let prefix = format!("ContourSmoother::phase_{}.", i(&fx["phase"]));
    for (k, v) in [("cg_line_search_type", json!(1)), ("cg_iter_for_restart", json!(0)), ("cg_max_iter", json!(0)),
        ("cg_min_iter", json!(-1)), ("cg_knock_out_count_down", json!(-1)), ("cg_rel_tol", json!(null)), ("cg_abs_tol", json!(-1))] {
        let c = with_param(fx, &format!("{prefix}{k}"), v); let o = Owned::new(&c);
        assert!(smoothing_optimize(&mut o.input(&c), None).is_err(), "{k}");
    }
    let c = with_param(fx, "ContourSmoother::optimizer_type", json!([1, 1, 1])); let o = Owned::new(&c);
    assert!(smoothing_optimize(&mut o.input(&c), None).is_err());
    let o = Owned::new(fx);
    assert_same(&smoothing_optimize(&mut o.input(fx), None).unwrap().to_json(), &fx["expected"], "repeatable");
}

#[test]
fn seeing_blind_optimization_matches_all_441_native_controllers() {
    let reference = common::read_json("smoothing-cached-optimizer-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 441);
    let (mut variants, mut phases) = (BTreeSet::new(), BTreeSet::new());
    let (mut blind_runs, mut rejected, mut wrapped) = (0, 0, 0);
    for (index, fx) in cases.iter().enumerate() {
        let o = Owned::new(fx);
        let a = smoothing_optimize(&mut o.input(fx), None).unwrap();
        assert_same(&a.to_json(), &fx["expected"], &format!("seeing/blind case {index}"));
        variants.insert(i(&fx["cachedVariant"])); phases.insert(i(&fx["phase"]));
        if a.line_iterations < a.iterations { blind_runs += 1; }
        let p = |k: &str| i(&fx["parameters"][format!("ContourSmoother::phase_{}.{k}", i(&fx["phase"]))]);
        let calls = a.iterations + if a.iterations < p("cg_max_iter") { 1 } else { 0 };
        let seeing = p("cg_num_seeing_steps"); let cycle = seeing + p("cg_num_blind_steps");
        let scheduled = (calls / cycle) * seeing + (calls % cycle).min(seeing);
        if a.line_iterations > scheduled { rejected += 1; }
        if calls > cycle { wrapped += 1; }
    }
    assert_eq!(variants.into_iter().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(phases.into_iter().collect::<Vec<_>>(), vec![0, 1, 2]);
    assert!(blind_runs > 0);
    assert_eq!((rejected, wrapped), (187, 321));
}

#[test]
fn seeing_blind_controls_fail_before_mutation() {
    let reference = common::read_json("smoothing-cached-optimizer-reference.json");
    let fx = &reference["cases"][2];
    let prefix = format!("ContourSmoother::phase_{}.", i(&fx["phase"]));
    for (k, v) in [("cg_num_seeing_steps", json!(0)), ("cg_num_blind_steps", json!(-1)),
        ("cg_blind_blend_latest", json!(-0.1)), ("cg_blind_blend_latest", json!(1.1)), ("cg_blind_blend_latest", json!(null))] {
        let c = with_param(fx, &format!("{prefix}{k}"), v); let o = Owned::new(&c);
        assert!(smoothing_optimize(&mut o.input(&c), None).is_err(), "{k}");
    }
}
