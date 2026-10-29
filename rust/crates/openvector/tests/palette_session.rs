mod common;
use common::read_json;
use openvector::palette::{CoreSession,GeneratedPalette,AnalysisControls};
use serde_json::Value;
fn bytes(v:&Value)->Vec<u8>{v.as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect()}
fn numbers(v:&Value)->Vec<f64>{v.as_array().unwrap().iter().map(|v|v.as_f64().unwrap()).collect()}
fn compare(session:&CoreSession,actual:GeneratedPalette,expected:&Value){
    assert_eq!(actual.palette.iter().map(|v|*v as f64).collect::<Vec<_>>(),numbers(&expected["palette"]),"palette");
    assert_eq!(actual.indices,expected["indices"].as_array().unwrap().iter().map(|v|v.as_i64().unwrap() as i32).collect::<Vec<_>>(),"indices");
    assert_eq!(actual.weights,numbers(&expected["weights"]),"membership weights");
    assert_eq!(actual.passes,expected["passes"].as_u64().unwrap() as usize);assert_eq!(session.random_state(),expected["randomState"].as_u64().unwrap() as u32);
}
#[test]
fn native_refinement_sessions(){for(name,weighted,total)in [("refinement-reference.json",false,89),("weighted-refinement-reference.json",true,80)]{let reference=read_json(name);let cases=reference["cases"].as_array().unwrap();assert_eq!(cases.len(),total);let session=CoreSession::default();for fixture in cases{let bgra=bytes(&fixture["bgra"]);let palette=numbers(&fixture["palette"]);let actual=if weighted{session.refine_weighted(fixture["width"].as_i64().unwrap(),fixture["height"].as_i64().unwrap(),&bgra,&palette)}else{session.refine_unweighted(&bgra,&palette)}.unwrap();compare(&session,actual,&fixture["expected"]);}}}
#[test]
fn native_generation_sessions(){for(name,weighted,total)in [("generation-reference.json",false,74),("weighted-generation-reference.json",true,80)]{let reference=read_json(name);let cases=reference["cases"].as_array().unwrap();assert_eq!(cases.len(),total);let session=CoreSession::default();for fixture in cases{let bgra=bytes(&fixture["bgra"]);let count=fixture["colorCount"].as_u64().unwrap() as usize;let bins=fixture["bins"].as_u64().unwrap() as usize;let connected=fixture["connected"].as_bool().unwrap();let actual=if weighted{session.generate_weighted(&bgra,count,AnalysisControls{width:fixture["width"].as_u64().unwrap() as usize,height:fixture["height"].as_u64().unwrap() as usize,bins,connected,anti_alias:true,cost_per_color:0.15,edge_scale:50.0,edge_floor:0.01})}else{session.generate_unweighted(&bgra,count,bins,connected)}.unwrap();compare(&session,actual,&fixture["expected"]);}}}
#[test]
fn invalid_generation_does_not_consume_random_stream(){let session=CoreSession::default();assert!(session.generate_unweighted(&[0,0,0,255],0,8,false).is_err());assert!(session.generate_unweighted(&[0,0,0,255],2,9,false).is_err());assert!(session.refine_unweighted(&[0],&[0.0,0.0,0.0,1.0]).is_err());assert_eq!(session.random_state(),1);let shared=session.clone();session.next_random();assert_eq!(session.random_state(),shared.random_state());assert_eq!(CoreSession::default().random_state(),1);}

fn hex(value:&str)->Vec<u8>{(0..value.len()).step_by(2).map(|i|u8::from_str_radix(&value[i..i+2],16).unwrap()).collect()}
fn joined_hex(value:&Value)->Vec<u8>{value.as_array().unwrap().iter().flat_map(|v|hex(v.as_str().unwrap())).collect()}
#[test]
fn native_automatic_sessions_keep_scores_features_and_fatal_random_effects(){
    let mut session=CoreSession::default();let mut generator=CoreSession::default();let mut fault=false;let mut cases=0;let mut generated=0;let mut scored=0;
    for frame in common::read_ndjson("classification-features-reference.ndjson.gz"){
        if frame["type"]!="case"{continue}
        if fault{session=CoreSession::default();generator=CoreSession::default()}
        assert_eq!(session.random_state(),frame["initialRandomState"].as_u64().unwrap() as u32);
        assert_eq!(generator.random_state(),session.random_state());
        let input=&frame["input"];let bgra=bytes(&input["bgra"]);
        let controls=AnalysisControls{width:input["width"].as_u64().unwrap() as usize,height:input["height"].as_u64().unwrap() as usize,bins:input["bins"].as_u64().unwrap() as usize,connected:input["connected"].as_bool().unwrap(),anti_alias:input["antiAlias"].as_bool().unwrap(),cost_per_color:input["costPerColor"].as_f64().unwrap(),edge_scale:50.0,edge_floor:0.01};
        for candidate in frame["candidates"].as_array().unwrap(){
            let count=candidate["colorCount"].as_u64().unwrap() as usize;
            let actual=if controls.anti_alias{generator.generate_weighted(&bgra,count,controls)}else{generator.generate_unweighted(&bgra,count,controls.bins,controls.connected)}.unwrap();
            assert_eq!(actual.palette.iter().map(|v|*v as f64).collect::<Vec<_>>(),numbers(&candidate["palette"]));
            assert_eq!(actual.indices,candidate["indices"].as_array().unwrap().iter().map(|v|v.as_i64().unwrap() as i32).collect::<Vec<_>>());
            assert_eq!(actual.weights.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),joined_hex(&candidate["weightBits"]));
            assert_eq!(generator.random_state(),candidate["randomState"].as_u64().unwrap() as u32);generated+=1;
        }
        let analysis=session.analyze_color_counts(&bgra,controls);fault=frame.get("expectedFault").is_some_and(|v|!v.is_null()&&v!=&Value::Bool(false));
        if fault{assert!(analysis.unwrap_err().contains("Automatic color selection has no histogram peaks"))}
        else{
            let analysis=analysis.unwrap();
            assert_eq!(analysis.peak_weights.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),joined_hex(&frame["peakWeightBits"]));
            assert_eq!(analysis.features.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),joined_hex(&frame["featureBits"]));
            assert_eq!(serde_json::json!({"label":analysis.classification.label,"mode":analysis.classification.mode,"colorCount":analysis.classification.color_count}),frame["expected"]);
            for(actual,expected)in analysis.candidates.iter().zip(frame["candidates"].as_array().unwrap()){
                assert_eq!([actual.score.total,actual.score.error,actual.score.colors].iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),hex(expected["scoreBits"].as_str().unwrap()));
                assert_eq!(actual.generated.palette.iter().map(|v|*v as f64).collect::<Vec<_>>(),numbers(&expected["palette"]));
                assert_eq!(actual.generated.indices,expected["indices"].as_array().unwrap().iter().map(|v|v.as_i64().unwrap() as i32).collect::<Vec<_>>());
                assert_eq!(actual.generated.weights.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),joined_hex(&expected["weightBits"]));
                assert_eq!(actual.random_state,expected["randomState"].as_u64().unwrap() as u32);scored+=1;
            }
        }
        assert_eq!(session.random_state(),frame["randomState"].as_u64().unwrap() as u32);cases+=1;
    }
    assert_eq!((cases,generated,scored),(576,6336,2970));
}
