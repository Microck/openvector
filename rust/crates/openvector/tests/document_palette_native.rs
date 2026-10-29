mod common;
mod editor_support;
use openvector::palette::CoreSession;
use serde_json::json;
use editor_support::{hex,compare_settings,compare_json};
#[test]
fn retained_document_palette_caches_match_all_native_actions(){
    let session=CoreSession::default();let mut document=session.create_document();let mut cases=0;let mut actions=0;
    for frame in common::read_ndjson("document-palette-reference.ndjson.gz"){
        if frame["type"]=="metadata"{assert_eq!(frame["wholeNativeResponseRepeatIdentical"],true);continue}
        let bgra=common::b64(frame["bgra"].as_str().unwrap());document.set_source(frame["width"].as_u64().unwrap() as usize,frame["height"].as_u64().unwrap() as usize,&bgra,true,0.0).unwrap();
        compare_settings(&document.state().settings,frame["initialSettingsBits"].as_str().unwrap(),"");
        for(edit,expected)in frame["edits"].as_array().unwrap().iter().zip(frame["states"].as_array().unwrap()){
            let v=edit["values"].as_array().unwrap();let controls=json!({"lower":v[0],"upper":v[1],"category":v[2],"quality":v[3],"sensitivity":v[4],"advanced":v[5].as_i64().unwrap()!=0,"antiAlias":v[6].as_i64().unwrap()!=0,"backgroundMode":v[7],"background":edit["background"],"complexity":5,"minimumPixels":0,"artifactRejection":0,"clusterColors":true,"paletteMode":2,"palette":[],"smoothness":6,"detectCorners":true,"curveComplexity":6,"smoothingAntiAlias":true});
            let mut state=document.analyze(&controls,None).unwrap();assert_eq!(state.stage_keys.palette,expected["cacheKey"].as_str().unwrap());compare_json(&json!(state.completed),&expected["completed"],"completed");assert_eq!(state.scores.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),hex(expected["scoreBits"].as_str().unwrap()));
            for(count,palette)in state.palettes.iter().enumerate(){assert_eq!(palette.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),hex(expected["paletteBits"][count].as_str().unwrap()))}
            compare_settings(&state.settings,expected["settingsBits"].as_str().unwrap(),"");compare_json(&json!([state.status,state.progress as u32]),&json!([expected["status"],expected["progress"]]),"task result");assert_eq!(state.cancelled,expected["cancelled"].as_bool().unwrap());assert_eq!(state.processed,expected["processed"].as_bool().unwrap());assert_eq!(state.random_state,expected["randomState"].as_u64().unwrap() as u32);
            for(name,image)in [("source",&state.source),("working",&state.working)]{compare_json(&json!([image.width,image.height]),&expected[format!("{name}Dimensions")],name);assert_eq!(image.bgra,common::b64(expected[format!("{name}Bgra")].as_str().unwrap()))}
            assert_eq!(state.original.bgra,common::b64(frame["originalBgra"].as_str().unwrap()));
            state.source.bgra.fill(0);state.working.bgra.fill(0);state.original.bgra.fill(0);state.completed.fill(0);state.scores.fill(-1.0);for p in &mut state.palettes{p.fill(0.0)}state.settings.background.fill(0.0);state.parameters["Shared::is_anti_aliased"]=json!(-1);actions+=1;
        }cases+=1;
    }assert_eq!((cases,actions),(10,220));
}
