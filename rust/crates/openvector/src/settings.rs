//! Native parameter profiles and exact discrete advanced-control tables.
use serde_json::{json, Value};
use std::sync::LazyLock;
pub static NUMERIC_DATA: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(include_str!("../../../../src/parameter-data.json")).expect("checked-in parameter data"));
fn choice(v: &Value, name: &str, choices: &[&str]) -> Result<usize,String> {
    choices.iter().position(|s| v[name].as_str() == Some(*s)).ok_or_else(|| format!("{name} must be one of: {}.",choices.join(", ")))
}
fn integer(v: &Value,name:&str,min:i64,max:i64)->Result<i64,String>{
    v[name].as_f64().filter(|x| x.fract()==0.0 && *x>=min as f64 && *x<=max as f64).map(|x|x as i64).ok_or_else(||format!("{name} must be an integer from {min} to {max}."))
}
fn boolean(v:&Value,name:&str)->Result<bool,String>{v[name].as_bool().ok_or_else(||format!("{name} must be true or false."))}
pub fn resolve_parameters(options:&Value)->Result<Value,String>{
    if !options.is_object(){return Err("Settings must be an object.".into())}
    let stage=choice(options,"stage",&["palette","segmentation","smoothing","fitting","classification"])?;
    if stage==4{return Ok(NUMERIC_DATA["profiles"][0].clone())}
    let mode=choice(options,"mode",&["basic","advanced"])?;
    if mode==0 {
        let category=choice(options,"category",&["photograph","artwork-with-blending","artwork-without-blending","unclassified"])?;
        let quality=choice(options,"quality",&["high","medium","low"])?;
        if category==3 && stage!=0{return Err("Basic processing requires a classified image category.".into())}
        let mut p=NUMERIC_DATA["profiles"][if stage==0{0}else{9-3*category-quality}].clone();
        if stage==0{p["Shared::is_anti_aliased"]=json!(i32::from(category==1))} return Ok(p)
    }
    if stage==0 {
        let sensitivity=choice(options,"colorSensitivity",&["high","medium","low"])?;
        let aa=boolean(options,"antiAlias")?;
        let mut p=NUMERIC_DATA["profiles"][0].clone();
        p["Shared::is_anti_aliased"]=json!(i32::from(aa));p["PaletteFinder::use_16_hist_bins"]=json!(i32::from(sensitivity!=2));p["PaletteFinder::use_26_connected_peaks"]=json!(i32::from(sensitivity!=0));return Ok(p)
    }
    let mut p=NUMERIC_DATA["profiles"][5].clone();
    if stage==1 {
        let complexity=integer(options,"complexity",1,12)? as usize;
        let minimum=integer(options,"minimumPixels",0,i32::MAX as i64)?;
        let rejection=choice(options,"artifactRejection",&["none","moderate","aggressive"])?;
        let cluster=boolean(options,"clusterColors")?;
        for (i,k) in ["Segmenter::lambda_pre","Segmenter::lambda_initial","Segmenter::lambda_final"].iter().enumerate(){p[*k]=NUMERIC_DATA["segmentation"][complexity-1][i].clone()}
        p["Segmenter::min_num_pixels"]=json!(minimum);p["Shared::is_anti_aliased"]=json!(i32::from(rejection!=2));p["shadowing"]=json!(rejection==0);p["clusterColors"]=json!(cluster);return Ok(p)
    }
    let smoothness=integer(options,"smoothness",1,12)? as usize;
    let complexity=integer(options,"curveComplexity",1,12)? as usize;
    let aa=boolean(options,"antiAlias")?;let corners=boolean(options,"detectCorners")?;
    let smoothing=&NUMERIC_DATA["smoothing"][usize::from(aa)][smoothness-1];let thresholds=&NUMERIC_DATA["curves"][complexity-1];
    p["ContourSmoother::measurement_types"]=json!([i32::from(aa),i32::from(aa),i32::from(aa)]);p["ContourSmoother::prior_types"]=json!([3,3,3]);
    p["ContourSmoother::prior_strengths"]=smoothing["priorStrengths"].clone();p["ContourSmoother::length_penalty_weights"]=smoothing["lengthPenalties"].clone();
    p["ContourSmoother::do_puncture_corners"]=json!([0,i32::from(corners),0]);p["ContourSmoother::is_enabled"]=json!([1,i32::from(corners),1]);
    p["BezierFitter::stat_thresh_initial"]=thresholds[0].clone();p["BezierFitter::stat_thresh_final"]=thresholds[1].clone();p["Shared::is_anti_aliased"]=json!(i32::from(aa));Ok(p)
}
