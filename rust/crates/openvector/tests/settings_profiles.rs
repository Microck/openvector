mod common;
use common::read_json;
use openvector::{settings::resolve_parameters,settings_profile::*,saved_profiles::SavedAdvancedProfiles};
use serde_json::{json,Value};
#[test]
fn all_native_advanced_parameter_cases() {
    let reference = read_json("settings-reference.json");
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 648);
    for c in cases {
        if c["stage"] == "segmentation" {
            let rejection = ["none", "moderate", "aggressive"][c["rejection"].as_u64().unwrap() as usize];
            let p = resolve_parameters(&json!({
                "mode": "advanced", "stage": "segmentation", "complexity": c["complexity"],
                "minimumPixels": c["minimumPixels"], "artifactRejection": rejection,
                "clusterColors": c["cluster"]
            })).unwrap();
            assert_eq!(json!({
                "lambda": [p["Segmenter::lambda_pre"], p["Segmenter::lambda_initial"], p["Segmenter::lambda_final"]],
                "minimumPixels": p["Segmenter::min_num_pixels"],
                "antiAlias": p["Shared::is_anti_aliased"],
                "shadowing": i32::from(p["shadowing"].as_bool().unwrap()),
                "clusterColors": i32::from(p["clusterColors"].as_bool().unwrap())
            }), c["expected"]);
        } else {
            for stage in ["smoothing", "fitting"] {
                let p = resolve_parameters(&json!({
                    "mode": "advanced", "stage": stage, "smoothness": c["smoothness"],
                    "curveComplexity": c["complexity"], "antiAlias": c["antiAlias"],
                    "detectCorners": c["corners"]
                })).unwrap();
                assert_eq!(json!({
                    "measurementTypes": p["ContourSmoother::measurement_types"],
                    "priorTypes": p["ContourSmoother::prior_types"],
                    "priorStrengths": p["ContourSmoother::prior_strengths"],
                    "lengthPenalties": p["ContourSmoother::length_penalty_weights"],
                    "punctureCorners": p["ContourSmoother::do_puncture_corners"],
                    "enabled": p["ContourSmoother::is_enabled"],
                    "bezierThresholds": [p["BezierFitter::stat_thresh_initial"], p["BezierFitter::stat_thresh_final"]],
                    "antiAlias": p["Shared::is_anti_aliased"]
                }), c["expected"]);
            }
        }
    }
}
#[test]
fn basic_profiles_are_owned_and_label_mapped(){for(category_index,category)in ["photograph","artwork-with-blending","artwork-without-blending"].iter().enumerate(){for(quality_index,quality)in ["high","medium","low"].iter().enumerate(){let options=json!({"stage":"segmentation","mode":"basic","category":category,"quality":quality});let mut p=resolve_parameters(&options).unwrap();assert_eq!(p.as_object().unwrap().len(),100);assert_eq!(p["Shared::image_type_code"],json!(8-category_index*3-quality_index));p["ContourSmoother::prior_strengths"][0]=json!(-1);assert_ne!(p,resolve_parameters(&options).unwrap());}}}
#[test]
fn profile_bookkeeping_survives_copies_and_storage(){let mut p=default_selected_profile();p.color_count=-7;p.basic_color_count=21;p.palette_color_count=8;p.settings.palette=vec![0.1,0.2,0.3,0.4];p.settings.background=[0.1,0.2,0.3,0.4];let parsed=read_settings_profile(&p.to_json()).unwrap();assert_eq!(parsed,p);let stored=stored_settings_profile(&p);assert_eq!(stored["settings"]["palette"],json!([]));assert_eq!(stored["settings"]["background"][3],json!(1.0));let applied=apply_advanced_profile(Some(&default_batch_profile()),&p,true).unwrap();assert!(applied.settings.advanced);assert_eq!(applied.settings.category,3);assert_eq!(applied.color_count,-7);assert_eq!(applied.basic_color_count,0);assert_eq!(applied.palette_color_count,8);assert_eq!(applied.settings.palette,p.settings.palette);let mut invalid=p.to_json();invalid["colorCount"]=json!(2147483648u64);assert!(read_settings_profile(&invalid).is_err());}
#[test]
fn saved_profiles_order_capacity_rename_and_restart(){let mut saved=SavedAdvancedProfiles::default();let mut p=default_advanced_profile();p.color_count=9;saved.put("custom",p.clone());saved.put("other",p.clone());assert_eq!(saved.select("custom").0,1);assert_eq!(saved.snapshot()[0].name,"custom");assert!(saved.rename("custom","other",false).is_err());assert_eq!(saved.rename("custom","other",true).unwrap().replaced_index,1);assert_eq!(saved.select("missing"),(-1,None));saved.rename("other",DEFAULT_ADVANCED_PROFILE_NAME,true).unwrap();assert_eq!(saved.snapshot()[0].profile.color_count,9);let encoded=Value::Array(saved.snapshot().iter().map(|e|json!({"name":e.name,"profile":e.profile.to_json()})).collect());let restored=SavedAdvancedProfiles::new(&encoded).unwrap();assert_eq!(restored.snapshot()[0].profile,default_advanced_profile());for i in 0..210{saved.put(&i.to_string(),p.clone())}assert_eq!(saved.len(),200);saved.reset();assert_eq!(saved.snapshot()[0].profile,default_advanced_profile());}
#[test]
fn profile_file_round_trip_uses_stored_colors(){let path=std::env::temp_dir().join(format!("openvector-profile-test-{}.json",std::process::id()));let mut saved=SavedAdvancedProfiles::default();let mut p=default_advanced_profile();p.settings.palette=vec![0.0,0.0,0.0,1.0];p.settings.background=[0.125,0.5,0.75,0.25];saved.put("fixture",p.clone());saved.write_file(&path).unwrap();let restored=SavedAdvancedProfiles::read_file(&path).unwrap();std::fs::remove_file(path).unwrap();let actual=restored.snapshot().into_iter().find(|e|e.name=="fixture").unwrap();assert_eq!(actual.profile,read_settings_profile(&stored_settings_profile(&p)).unwrap());}
