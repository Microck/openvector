mod common;
use serde_json::Value;
use openvector::region_data::Raster;
use openvector::segmentation::*;
fn bytes(v:&Value)->Vec<u8>{v.as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect()}
fn ints(v:&Value)->Vec<i64>{v.as_array().unwrap().iter().map(|v|v.as_i64().unwrap()).collect()}
fn floats(v:&Value)->Vec<f64>{v.as_array().unwrap().iter().map(|v|v.as_f64().unwrap()).collect()}
fn mode(v:&Value)->OptimizerMode{match v.as_str().unwrap(){"axis"=>OptimizerMode::Axis,"eight"=>OptimizerMode::Eight,"small"=>OptimizerMode::Small,_=>panic!("invalid fixture mode")}}
fn optimizer(f:&Value)->RegionOptimizer{RegionOptimizer::new(f["width"].as_i64().unwrap(),f["height"].as_i64().unwrap(),&bytes(&f["bgra"]),&ints(&f["regions"]),f["finalLambda"].as_f64().unwrap(),f["minimumPixels"].as_i64().unwrap()).unwrap()}
fn assert_regions(actual:&RegionSnapshot,e:&Value,label:&str){
    assert_eq!(actual.indices.iter().map(|&v|v as i64).collect::<Vec<_>>(),ints(&e["indices"]),"{label} indices");
    assert_eq!(actual.counts.iter().map(|&v|v as i64).collect::<Vec<_>>(),ints(&e["counts"]),"{label} counts");
    assert_eq!(actual.sums.iter().map(|&v|v as f64).collect::<Vec<_>>(),floats(&e["sums"]),"{label} sums");
    assert_eq!(actual.color_indices.iter().map(|&v|v as i64).collect::<Vec<_>>(),ints(&e["colorIndices"]),"{label} colors");
    assert_eq!(actual.active_count,e["activeCount"].as_u64().unwrap() as usize,"{label} active");
}
fn assert_segment(actual:&SegmentResult,e:&Value,label:&str){
    assert_eq!(actual.indices.iter().map(|&v|v as i64).collect::<Vec<_>>(),ints(&e["indices"]),"{label} indices");
    assert_eq!(actual.counts.iter().map(|&v|v as i64).collect::<Vec<_>>(),ints(&e["counts"]),"{label} counts");
    assert_eq!(actual.palette.iter().map(|&v|v as f64).collect::<Vec<_>>(),floats(&e["palette"]),"{label} palette");
    assert_eq!(actual.color_indices.iter().map(|&v|v as i64).collect::<Vec<_>>(),ints(&e["colorIndices"]),"{label} colors");
}
#[test]
fn all_1042_native_merge_costs(){let data=common::read_json("merge-cost-reference.json");let cases=data["cases"].as_array().unwrap();assert_eq!(cases.len(),1042);for (i,f) in cases.iter().enumerate(){let counts: [i64;2]=ints(&f["counts"]).try_into().unwrap();let sums=[floats(&f["sums"][0]).try_into().unwrap(),floats(&f["sums"][1]).try_into().unwrap()];assert_eq!(region_color_merge_cost(counts,sums).unwrap() as f64,f["expected"].as_f64().unwrap(),"merge {i}");}}
#[test]
fn all_300_native_recursive_tile_maps(){let data=common::read_json("initial-regions-reference.json");let cases=data["cases"].as_array().unwrap();assert_eq!(cases.len(),300);for (i,f) in cases.iter().enumerate(){let labels=initialize_regions(f["width"].as_i64().unwrap(),f["height"].as_i64().unwrap(),&bytes(&f["bgra"]),f["threshold"].as_f64().unwrap()).unwrap();assert_eq!(labels.iter().map(|&v|v as i64).collect::<Vec<_>>(),ints(&f["expected"]),"tile {i}");}}
#[test]
fn all_945_native_optimizer_passes(){let data=common::read_json("region-pass-reference.json");let cases=data["cases"].as_array().unwrap();assert_eq!(cases.len(),945);for(i,f)in cases.iter().enumerate(){let mut model=optimizer(f);model.iterate(f["lambda"].as_f64().unwrap(),mode(&f["mode"]),None).unwrap();assert_regions(&model.snapshot(),&f["expected"],&format!("pass {i}"));}}
#[test]
fn all_994_native_diagonal_repairs(){let data=common::read_json("diagonal-contacts-reference.json");let cases=data["cases"].as_array().unwrap();assert_eq!(cases.len(),994);let mut changed=0;for(i,f)in cases.iter().enumerate(){let mut model=optimizer(f);model.iterate(f["lambda"].as_f64().unwrap(),mode(&f["mode"]),None).unwrap();let before=model.snapshot();assert_eq!(before.indices.iter().map(|&v|v as i64).collect::<Vec<_>>(),ints(&f["beforeRepair"]));model.repair_diagonal_contacts();let after=model.snapshot();changed+=usize::from(before.indices!=after.indices);assert_regions(&after,&f["expected"],&format!("diagonal {i}"));}assert!(changed>0);}
#[test]
fn all_632_native_presegmentation_states(){let data=common::read_json("presegmentation-reference.json");let cases=data["cases"].as_array().unwrap();assert_eq!(cases.len(),632);for(i,f)in cases.iter().enumerate(){let pixels=bytes(&f["bgra"]);let raster=Raster{width:f["width"].as_u64().unwrap() as usize,height:f["height"].as_u64().unwrap() as usize,bgra:&pixels};assert_regions(&presegment_regions(raster,&f["parameters"]).unwrap(),&f["expected"],&format!("presegment {i}"));}}
#[test]
fn complete_native_segmentation_corpora(){
    for (name,count) in [("unweighted-segmentation-reference.json",160),("unweighted-diagonal-segmentation-reference.json",168),("anti-alias-segmentation-reference.json",240),("clustered-anti-alias-segmentation-reference.json",240),("pipeline-segmentation-reference.json",15),("supplied-palette-segmentation-reference.json",782)]{
        let data=common::read_json(name);let cases=data["cases"].as_array().unwrap();assert_eq!(cases.len(),count);
        if name=="supplied-palette-segmentation-reference.json"{assert_eq!(data["unsupportedCases"].as_array().unwrap().len(),482);}
        for(i,f)in cases.iter().enumerate(){let pixels=bytes(&f["bgra"]);let raster=Raster{width:f["width"].as_u64().unwrap() as usize,height:f["height"].as_u64().unwrap() as usize,bgra:&pixels};let palette=f.get("palette").map(|v|floats(v).into_iter().map(|v|v as f32).collect::<Vec<_>>());let actual=segment(raster,&f["parameters"],palette.as_deref()).unwrap();assert_segment(&actual,&f["expected"],&format!("{name} {i}"));if let Some(palette)=palette{assert_eq!(actual.palette,palette);}}
    }
}
#[test]
fn all_578_native_shadow_dispatches(){let rows=common::read_ndjson("shadow-segmentation-reference.ndjson.gz");let mut count=0;for f in &rows{if f["type"]!="case"{continue;}let pixels=bytes(&f["bgra"]);let raster=Raster{width:f["width"].as_u64().unwrap() as usize,height:f["height"].as_u64().unwrap() as usize,bgra:&pixels};let palette=f.get("palette").map(|v|floats(v).into_iter().map(|v|v as f32).collect::<Vec<_>>());let actual=segment(raster,&f["parameters"],palette.as_deref()).unwrap();assert_segment(&actual,&f["expected"],f["id"].as_str().unwrap());count+=1;}assert_eq!(count,578);}
#[test]
fn optimizer_validation_ownership_and_projection(){let pixels=[0,0,0,255,255,255,255,255];let mut optimizer=RegionOptimizer::new(2,1,&pixels,&[0,1],2.0,0).unwrap();let before=optimizer.snapshot();assert!(optimizer.iterate(-1.0,OptimizerMode::Axis,None).is_err());assert!(optimizer.iterate(1.0,OptimizerMode::Small,Some(-1)).is_err());assert_eq!(optimizer.snapshot(),before);let mut leaked=optimizer.snapshot();leaked.indices.fill(7);assert_eq!(optimizer.snapshot(),before);optimizer.iterate(2.0,OptimizerMode::Axis,None).unwrap();assert!(optimizer.project_palette(false).is_err());optimizer.rebuild();let palette=optimizer.project_palette(false).unwrap();assert_eq!(palette,vec![128.0f32*(1.0/255.0),128.0f32*(1.0/255.0),128.0f32*(1.0/255.0),1.0]);assert!(optimizer.project_palette(false).is_err());optimizer.rebuild();assert_eq!(optimizer.snapshot().color_indices,vec![0]);assert!(initialize_regions(0,1,&[],0.0).is_err());assert!(region_color_merge_cost([i32::MAX as i64,1],[[0.0;4];2]).is_err());}
#[test]
fn live_state_retains_separate_exports_and_cancellation_parameter_write(){
    use openvector::segmentation_state::*;
    let data=common::read_json("unweighted-diagonal-segmentation-reference.json");let f=&data["cases"][0];let pixels=bytes(&f["bgra"]);let raster=Raster{width:f["width"].as_u64().unwrap() as usize,height:f["height"].as_u64().unwrap() as usize,bgra:&pixels};
    let old=SegmentationSnapshot{width:raster.width,height:raster.height,indices:Some(vec![9;raster.width*raster.height]),palette:vec![0.25;4],counts:vec![99],color_indices:vec![3]};let mut state=SegmentationState::new(Some(&old),raster.width,raster.height,None);assert_eq!(state.snapshot(),&old);let mut parameters=f["parameters"].clone();let error=state.run(raster,&mut parameters,None,&mut |event,_|{if event.phase==3&&event.substep==1{Err("cancelled".into())}else{Ok(())}}).unwrap_err();assert_eq!(error,"cancelled");assert_eq!(parameters["Segmenter::min_num_pixels"],0);assert_eq!(state.snapshot().counts,vec![99]);assert_eq!(state.snapshot().color_indices,vec![3]);assert_ne!(state.snapshot().indices,old.indices);
    let changed=SegmentationState::new(Some(&old),raster.width+1,raster.height,None);assert!(changed.snapshot().indices.is_none());assert_eq!(changed.snapshot().counts,old.counts);
}
