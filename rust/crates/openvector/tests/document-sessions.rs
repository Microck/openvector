mod common;
#[path = "editor_support/mod.rs"]
mod editor_support;
use serde_json::{json, Value};
use openvector::{palette::CoreSession, image_document::{DocumentSnapshot, ImageDocument}};
use editor_support::{compare_json, compare_settings, hex};
use sha2::{Digest, Sha256};

fn bytes32(values: &[f32]) -> Vec<u8> { values.iter().flat_map(|v| v.to_le_bytes()).collect() }
fn bytes64(values: &[f64]) -> Vec<u8> { values.iter().flat_map(|v| v.to_le_bytes()).collect() }
fn i32_at(bits: &str, offset: usize) -> i32 { i32::from_le_bytes(hex(bits)[offset..offset+4].try_into().unwrap()) }
fn attempt(failures: &mut Vec<String>, label: &str, f: impl FnOnce()) {
    if let Err(error) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        let message = error.downcast_ref::<String>().cloned().or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "non-string panic".into());
        failures.push(format!("{label}: {message}"));
    }
}
fn finish(failures: Vec<String>) { assert!(failures.is_empty(), "{} native replay failures:\n{}", failures.len(), failures.join("\n")); }
fn classification(state: &DocumentSnapshot) -> Value { let c=&state.classification; json!([c.category,c.quality,c.palette_mode,c.color_count]) }
fn parameters(actual: &Value, expected: &Value, registry: &Value) {
    assert_eq!(registry.as_object().unwrap().len(),100);
    for (name, descriptor) in registry.as_object().unwrap() {
        let scalar = [&actual[name]];
        let values: Vec<&Value> = actual[name].as_array().map(|v|v.iter().collect()).unwrap_or_else(||scalar.to_vec());
        assert_eq!(values.len(),descriptor["count"].as_u64().unwrap() as usize,"{name}");
        let mut encoded=Vec::new();
        for value in values { let n=value.as_f64().unwrap(); match descriptor["kind"].as_str().unwrap() {
            "float64"=>encoded.extend_from_slice(&n.to_le_bytes()),
            "float32"=>encoded.extend_from_slice(&(n as f32).to_le_bytes()),
            "int32"=>encoded.extend_from_slice(&(n as i32).to_le_bytes()),
            kind=>panic!("unknown parameter kind {kind}"),
        }}
        assert_eq!(encoded,hex(expected[name].as_str().unwrap()),"{name} parameter bits");
    }
}
fn compare_retained(state: &DocumentSnapshot, native: &Value, registry: &Value, action: usize, classified: bool) {
    compare_settings(&state.settings,native["settingsBits"].as_str().unwrap(),native["committedPaletteArgbBits"].as_str().unwrap());
    compare_json(&json!([state.status,state.progress,state.cancelled,state.processed,state.random_state]),&json!([native["status"],native["progress"],native["cancelled"],native["processed"],native["randomState"]]),"document status");
    assert_eq!(state.stage_keys.palette,native["cacheKey"].as_str().unwrap());
    let mut snapshot=state.to_json();
    let float=|v:f64| if v.is_finite(){json!(v)}else{json!({"float64Bits":v.to_le_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>()})};
    if let Some(contours)=&state.contours {for (i,node) in contours.nodes.iter().enumerate(){snapshot["contours"]["nodes"][i]["x"]=float(node.x);snapshot["contours"]["nodes"][i]["y"]=float(node.y);}}
    if let Some(fitting)=&state.fitting {
        for (i,node) in fitting.geometry.nodes.iter().enumerate(){snapshot["fitting"]["geometry"]["nodes"][i]["x"]=float(node.x);snapshot["fitting"]["geometry"]["nodes"][i]["y"]=float(node.y);}
        for (i,cubic) in fitting.geometry.cubics.iter().enumerate(){for (j,&v) in cubic.points.iter().enumerate(){snapshot["fitting"]["geometry"]["cubics"][i]["points"][j]=float(v);}}
    }
    for (key,value) in native["stageKeys"].as_object().unwrap() { compare_json(&snapshot["stageKeys"][key],value,key); }
    assert_eq!(json!(state.completed),native["completed"]);
    assert_eq!(bytes32(&state.scores),hex(native["scoreBits"].as_str().unwrap()));
    assert_eq!(state.palettes.len(),25);
    for (count,palette) in state.palettes.iter().enumerate() { assert_eq!(bytes32(palette),hex(native["paletteBits"][count].as_str().unwrap()),"palette {count}"); }
    for (kind,image) in [("source",&state.source),("working",&state.working)] {
        assert_eq!(json!([image.width,image.height]),native[format!("{kind}Dimensions")]);
        assert_eq!(image.bgra,common::b64(native[format!("{kind}Bgra")].as_str().unwrap()));
    }
    let p=&state.parameters;
    let shared=[p["usePalette"].as_bool().unwrap() as u32,p["Shared::is_anti_aliased"].as_f64().unwrap() as u32,0,p["shadowing"].as_bool().unwrap() as u32,p["clusterColors"].as_bool().unwrap() as u32,p["Shared::image_type_code"].as_f64().unwrap() as u32];
    assert_eq!(shared.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),hex(native["sharedBits"].as_str().unwrap()));
    compare_json(&p["shadowControl"],&native["shadowControl"],"shadow control");
    parameters(p,&native["parameterBits"],registry);
    if action==1 || (classified && state.segmentation.is_some()) {
        let map=state.segmentation.as_ref().unwrap();let expected=&native["segmentation"];
        assert_eq!(json!([map.width,map.height]),json!([expected["width"],expected["height"]]));
        assert_eq!(map.indices.as_ref().unwrap().iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),hex(expected["indexBits"].as_str().unwrap()));
        assert_eq!(bytes32(&map.palette),hex(expected["paletteBits"].as_str().unwrap()));
        assert_eq!(json!(map.counts),expected["counts"]);assert_eq!(json!(map.color_indices),expected["colorIndices"]);
    }
    if if classified { native["geometryPresent"]==true } else { action>=2 } {
        compare_json(&snapshot["contours"],&native["contours"],"complete contours");
        let s=state.smoothing.as_ref().unwrap();let e=&native["smoothingMetadata"];
        assert_eq!(json!(s.border_counts),e["borderCounts"]);assert_eq!(json!(s.penalty_counts),e["penaltyCounts"]);
        let refs:Vec<f32>=e["referencePositions"].as_array().unwrap().iter().map(|v|v.as_f64().unwrap() as f32).collect();assert_eq!(bytes32(&s.reference_positions),bytes32(&refs));
        if if classified { native["fittingPresent"]==true } else { action==3 } {
            compare_json(&snapshot["fitting"]["geometry"],&native["fittedGeometry"],"fitted geometry");
            compare_json(&snapshot["fitting"]["outputChains"],&native["outputChains"],"assembled chains");
        }
    }
    if classified { assert_eq!(classification(state),native["classificationFields"]); }
}
fn damage_snapshot(state: &mut DocumentSnapshot) {
    state.original.bgra.fill(0);state.source.bgra.fill(0);state.working.bgra.fill(0);
    state.completed.fill(0);state.scores.fill(-1.0);for p in &mut state.palettes {p.fill(0.0)}
    state.settings.background.fill(0.0);state.settings.palette.fill(0.0);state.classification.category=99;
    state.stage_keys.segmentation="caller edit".into();state.parameters["ContourSmoother::prior_types"]=json!([-1,-1,-1]);
    if let Some(s)=&mut state.segmentation {if let Some(i)=&mut s.indices{i.fill(-1)}s.palette.fill(f32::NAN);s.counts.fill(0);s.color_indices.fill(0);}
    if let Some(c)=&mut state.contours {if let Some(n)=c.nodes.first_mut(){n.x=f64::NAN}}
    if let Some(s)=&mut state.smoothing {s.border_counts.fill(-1);s.reference_positions.fill(f32::NAN);s.penalty_counts.fill(255);}
    if let Some(f)=&mut state.fitting {if let Some(n)=f.geometry.nodes.first_mut(){n.x=f64::NAN}if let Some(c)=&mut f.output_chains {if let Some(child)=c.children.first_mut(){child.push(usize::MAX)}}}
}
fn retained(corpus: &str, expected_counts: (usize,usize,usize,usize)) {
    let rows=common::read_ndjson(&format!("{corpus}-reference.ndjson.gz"));assert_eq!(rows[0]["wholeNativeResponseRepeatIdentical"],true);
    let controller=rows[0]["operation"].as_u64().unwrap()>=102;let classified=corpus=="document-classification";
    let mut document=CoreSession::default().create_document();let mut failures=Vec::new();let mut counts=(0,0,0,0);
    for frame in &rows[1..] {
        let id=frame["id"].as_str().unwrap();let bgra=common::b64(frame["bgra"].as_str().unwrap());let before=bgra.clone();
        document.set_source(frame["width"].as_u64().unwrap() as usize,frame["height"].as_u64().unwrap() as usize,&bgra,true,if classified{frame["shadowControl"].as_f64().unwrap()}else{0.0}).unwrap();
        attempt(&mut failures,id,|| {compare_settings(&document.snapshot().settings,frame["initialSettingsBits"].as_str().unwrap(),"");if classified{assert_eq!(classification(&document.snapshot()),frame["initialClassificationFields"]);}});
        let mut previous=[String::new(),String::new(),String::new()];
        for (index,step) in frame["steps"].as_array().unwrap().iter().enumerate() {
            let action=step["action"].as_u64().unwrap() as usize;let stage=["palette","segmentation","smoothing","fitting","classification"][action];let captured=&frame["states"][index];let native=&captured["state"];
            let mut controls=json!({"smoothness":6,"detectCorners":true,"curveComplexity":6,"smoothingAntiAlias":true});
            controls.as_object_mut().unwrap().extend(step.as_object().unwrap().clone());
            controls["palette"]=Value::Array(step["palette"].as_array().unwrap().iter().flat_map(|p|p.as_array().unwrap().iter().cloned()).collect());
            if classified {controls["extension"]=frame["extension"].clone()}
            let controls_before=controls.clone();let label=format!("{id}/{}",step["id"]);
            if action==1 {if captured["needsWork"]==true{counts.2+=1}else{counts.3+=1}}
            attempt(&mut failures,&label,|| {
                let mut state=if controller {
                    let mut task=document.run_task(stage,&controls,None).unwrap();let c=&captured["controller"];
                    assert_eq!(task.did_work,captured["needsWork"].as_bool().unwrap());
                    compare_settings(&task.settings,c["activeSettingsBits"].as_str().unwrap(),c["activePaletteArgbBits"].as_str().unwrap());
                    assert_eq!(c["activeSettingsBits"],captured["requestedSettingsBits"]);assert_eq!(c["pendingSettingsBits"],captured["requestedSettingsBits"]);
                    let work=i32::from(task.did_work);assert_eq!(c["info"],json!([work,if task.did_work{if action==4{1}else{action+2}}else{0},0,work,0,0,0,work,0,1]));
                    task.settings.background.fill(f32::NAN);task.settings.palette.fill(f32::NAN);
                    if action==4 {
                        assert!(task.did_work);let a=task.analysis.as_mut().unwrap();let r=&native["classificationAnalysis"];
                        let ic=a.image_analysis.classification;let pc=a.palette_analysis.classification;
                        assert_eq!(json!([ic.image_type,ic.quality,pc.mode,pc.color_count]),native["classificationFields"]);
                        assert_eq!(bytes32(&a.image_analysis.features),hex(r["imageFeatureBits"].as_str().unwrap()));assert_eq!(bytes64(&a.palette_analysis.features),hex(r["paletteFeatureBits"].as_str().unwrap()));
                        assert_eq!(a.palette_analysis.peak_weights.len(),r["peakCount"].as_u64().unwrap() as usize);assert_eq!(bytes64(&a.palette_analysis.peak_weights),hex(r["peakBits"].as_str().unwrap()));assert_eq!(a.palette_analysis.candidates.len(),11);
                        for (i,candidate) in a.palette_analysis.candidates.iter().enumerate() {let e=&r["candidates"][i];assert_eq!(candidate.color_count,e["count"].as_u64().unwrap() as usize);assert_eq!(bytes32(&candidate.generated.palette),hex(e["paletteBits"].as_str().unwrap()));assert_eq!(bytes64(&candidate.generated.weights),hex(e["weightBits"].as_str().unwrap()));assert_eq!(bytes32(&[a.recommendation_scores[i],candidate.score.error,candidate.score.colors]),hex(e["scoreBits"].as_str().unwrap()));}
                        let last=a.palette_analysis.candidates.last_mut().unwrap();assert_eq!(last.generated.indices.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),hex(native["segmentation"]["indexBits"].as_str().unwrap()));assert_eq!(bytes32(&last.generated.palette),hex(native["segmentation"]["paletteBits"].as_str().unwrap()));
                        for (x,y) in [(&a.image.source,&task.state.source),(&a.image.working,&task.state.working)] {assert_eq!((x.width,x.height,&x.bgra),(y.width,y.height,&y.bgra));}assert_eq!(a.random_state,task.state.random_state);
                        last.generated.indices.fill(-1);last.generated.palette.fill(f32::NAN);a.image.source.bgra.fill(0);a.image.working.bgra.fill(0);a.parameters["ContourSmoother::prior_types"]=json!([-1,-1,-1]);
                    }
                    task.state
                }else{match action{0=>document.analyze(&controls,None),1=>document.segment(&controls,None),2=>document.smooth(&controls,None),3=>document.fit(&controls,None),_=>unreachable!()}.unwrap()};
                if (1..=3).contains(&action) {let keys=[&state.stage_keys.segmentation,&state.stage_keys.smoothing,&state.stage_keys.fitting];assert_eq!(previous[action-1]!=*keys[action-1],captured["needsWork"].as_bool().unwrap());}
                compare_retained(&state,native,&rows[0]["parameters"],action,classified);
                assert_eq!(state.original.bgra,common::b64(frame["originalBgra"].as_str().unwrap()));assert_eq!(bgra,before);assert_eq!(controls,controls_before);
                if counts.0==0 && step["id"]=="flexible-initial" {for invalid in [json!({"complexity":0}),json!({"minimumPixels":-1}),json!({"paletteMode":1,"palette":[]}),json!({"background":[0,0,null,1]}),json!({"palette":[0,null,0,1]})] {let mut input=controls.clone();input.as_object_mut().unwrap().extend(invalid.as_object().unwrap().clone());assert!(document.segment(&input,None).is_err());compare_json(&document.snapshot().to_json(),&state.to_json(),"invalid edit preserves state");}}
                let owned=document.snapshot().to_json();damage_snapshot(&mut state);compare_json(&document.snapshot().to_json(),&owned,"snapshot ownership");
            });
            let state=document.snapshot();previous=[state.stage_keys.segmentation,state.stage_keys.smoothing,state.stage_keys.fitting];counts.1+=1;
        }
        counts.0+=1;
    }
    assert_eq!(counts,expected_counts);finish(failures);
}
#[test] fn document_segmentation(){retained("document-segmentation",(4,145,93,36));}
#[test] fn document_vector_stages(){retained("document-vector-stages",(8,378,44,5));}
#[test] fn document_task_controller(){retained("document-task-controller",(8,384,44,6));}
#[test] fn document_classification(){retained("document-classification",(19,380,57,0));}

fn load_encoded(document: &mut ImageDocument, encoded: &[u8], name: &str) -> String {
    let image=openvector::image_input::decode_image(encoded).unwrap();
    let filename=openvector::image_input::resolve_image_filename(name);
    document.set_source(image.width,image.height,&image.bgra,image.has_alpha,filename.shadow_control as f64).unwrap();
    filename.extension
}
fn automatic_session(index: usize) {
    let rows=common::read_ndjson("document-automatic-reference.ndjson.gz");
    assert_eq!(rows.len(),14);assert_eq!(rows[0]["wholeNativeResponseRepeatIdentical"],true);
    assert_eq!(json!([rows[0]["cases"],rows[0]["automaticActions"],rows[0]["parameterComparisons"]]),json!([13,36,3600]));
    let frame=&rows[index+1];let mut document=CoreSession::default().create_document();
    let mut controls=editor_support::clean_startup_profile();let mut selected=2;let mut palette_count=0;let mut recommendation=0;
    if frame["profile"]=="restored" {
        controls.as_object_mut().unwrap().extend(json!({"category":1,"quality":0,"sensitivity":2,"complexity":12,"smoothness":4,"curveComplexity":12,"backgroundMode":0,"background":[1,1,1,1]}).as_object().unwrap().clone());selected=3;
    }
    let mut loaded=false;let mut extension=String::new();let mut digest=String::new();let mut failures=Vec::new();
    for (i,step) in frame["steps"].as_array().unwrap().iter().enumerate() {
        let label=format!("{}/step {i}",frame["id"]);
        if step["action"]=="load" {
            let name=step["source"].as_str().unwrap();let encoded=common::b64(frame["inputs"][name].as_str().unwrap());
            digest=format!("{:x}",Sha256::digest(&encoded));extension=load_encoded(&mut document,&encoded,name);
            if loaded {controls["palette"]=json!([]);controls["backgroundMode"]=json!(-1);controls["background"]=json!([1,1,1,0]);palette_count=2;recommendation=-1;}
            loaded=true;
            if !step["expected"].is_null() {attempt(&mut failures,&label,||{
                let e=&step["expected"];let bits=e["selectedSettingsBits"].as_str().unwrap();let state=document.snapshot();
                assert_eq!(digest,e["sourceSha256"].as_str().unwrap());
                compare_settings(&openvector::document_controls::read_document_settings(&controls).unwrap(),bits,e["selectedPaletteArgbBits"].as_str().unwrap());
                assert_eq!((selected,palette_count,recommendation),(i32_at(bits,8),i32_at(bits,56),i32_at(bits,88)));
                assert_eq!(json!([state.original.width,state.original.height]),json!([e["header"][10],e["header"][11]]));assert_eq!(state.original.bgra,common::b64(e["originalBgra"].as_str().unwrap()));
                assert_eq!(json!(state.classification.category),e["documentClassification"][0]);assert_eq!(e["recommendedControls"],json!([-1,-1,-1]));
                assert_eq!(json!([state.progress,i32::from(state.cancelled),state.random_state]),json!([e["header"][12],e["header"][13],e["header"][14]]));
            });}
        }else{
            assert_eq!(step["action"],"auto");controls["extension"]=json!(extension);
            attempt(&mut failures,&label,|| {
                let mut result=document.run_automatic(&controls).unwrap();
                controls=Value::Object(result.settings.to_json());if let Some(count)=result.palette_count {selected=count as i32;palette_count=count as i32;}recommendation=result.recommended_background_mode;
                let e=&step["expected"];let bits=e["selectedSettingsBits"].as_str().unwrap();
                assert_eq!(digest,e["encodedFileSha256"].as_str().unwrap());compare_settings(&result.settings,bits,e["selectedPaletteArgbBits"].as_str().unwrap());
                assert_eq!((selected,palette_count,recommendation),(i32_at(bits,8),i32_at(bits,56),i32_at(bits,88)));
                assert_eq!(classification(&result.state),e["documentClassification"]);let c=&result.state.classification;assert_eq!(json!([c.category,c.quality,c.palette_mode]),e["recommendedControls"]);assert_eq!(e["controllerCompletion"],json!([0,0,2,12]));
                assert_eq!(e["parameters"].as_object().unwrap().len(),100);editor_support::compare_document_state(&result.state,&e["state"],&e["parameters"]);
                let before=document.snapshot().to_json();result.settings.palette.fill(f32::NAN);damage_snapshot(&mut result.state);compare_json(&document.snapshot().to_json(),&before,"automatic ownership");
            });
        }
    }
    finish(failures);
}
macro_rules! automatic_tests {($($name:ident:$index:expr),* $(,)?)=>{$(#[test] fn $name(){automatic_session($index);})*};}
automatic_tests!(automatic_pilot:0,automatic_three_colors:1,automatic_soft_disc:2,automatic_opaque_png:3,automatic_binary_alpha:4,automatic_partial_alpha:5,automatic_binary_blended:6,automatic_flexible_source:7,automatic_anchored_texture:8,automatic_anchored_texture_large:9,automatic_retained_session:10,automatic_transparent_session:11,automatic_clean_session:12);

fn batch_session(index: usize) {
    let rows=common::read_ndjson("batch-image-reference.ndjson.gz");assert_eq!(rows.len(),10);let frame=&rows[index+1];let mut document=CoreSession::default().create_document();let mut result=None;let mut failures=Vec::new();
    for source in frame["sources"].as_array().unwrap() {
        let name=source["name"].as_str().unwrap();
        let extension=load_encoded(&mut document,&common::b64(source["bytes"].as_str().unwrap()),name);
        let mut controls=frame["controls"].clone();controls["extension"]=json!(extension);
        attempt(&mut failures,name,|| {
            let mut stages=Vec::new();let mut observe=|stage:&str,task:&openvector::image_document::TaskResult,selection:&openvector::image_document::AutomaticSelection| {
                stages.push(stage.to_string());let mut owned=task.clone();damage_snapshot(&mut owned.state);owned.settings.background.fill(f32::NAN);let mut selected=selection.clone();selected.settings.palette.fill(f32::NAN);Ok(())
            };
            result=Some(document.run_batch_image_with_observers(&controls,None,Some(&mut observe)).unwrap());let state=&result.as_ref().unwrap().state;
            assert_eq!(json!(stages),frame["expectedStages"]);assert_eq!(state.progress,100);
            let mut input=openvector::vector_output::VectorOutputInput::new(&state.fitting.as_ref().unwrap().geometry,state.original.width as i64,state.original.height as i64);input.layout=openvector::export_options::Layout::GroupedCutouts;
            assert_eq!(openvector::svg::export_svg(&input).unwrap(),source["expectedSvg"].as_str().unwrap());
        });
    }
    attempt(&mut failures,"final batch state",||{
        let actual=result.as_ref().unwrap();let e=&frame["expectedDocument"];
        compare_settings(&actual.settings,e["controls"]["selectedSettingsBits"].as_str().unwrap(),e["selectedPaletteArgbBits"].as_str().unwrap());
        assert_eq!(actual.palette_count,if frame["expectedStages"].as_array().unwrap().contains(&json!("palette")){Some(e["selectedPaletteArgbBits"].as_str().unwrap().len()/32)}else{None});
        let c=classification(&actual.state);for (i,field) in e["controls"]["documentClassification"].as_array().unwrap().iter().enumerate(){assert_eq!(&c[i],field);}
        editor_support::compare_document_computation(&actual.state,&e["state"],&rows[0]["parameters"]);
    });finish(failures);
}
macro_rules! batch_tests {($($name:ident:$index:expr),* $(,)?)=>{$(#[test] fn $name(){batch_session($index);})*};}
batch_tests!(batch_uniform_pair:0,batch_logo_alpha_auto:1,batch_gradient_explicit:2,batch_logo_fixed_eight:3,batch_alpha_preserve:4,batch_alpha_auto_background:5,batch_name_collisions:6,batch_logo_flexible_count:7,batch_binary_auto_background:8);

#[test]
fn source_controls_resolve_independently() {
    let rows=common::read_ndjson("document-source-reference.ndjson.gz");assert_eq!(rows.len(),11);assert_eq!(rows[0]["wholeNativeResponseRepeatIdentical"],true);
    let mut actions=0;let mut failures=Vec::new();
    for frame in &rows[1..] {
        let width=frame["width"].as_u64().unwrap() as usize;let height=frame["height"].as_u64().unwrap() as usize;
        let bgra=common::b64(frame["bgra"].as_str().unwrap());let before=bgra.clone();
        let original=openvector::image_pipeline::prepare_original_image(width,height,&bgra,true).unwrap();assert_eq!(original.bgra,common::b64(frame["originalBgra"].as_str().unwrap()));let original_before=original.bgra.clone();
        for (i,edit) in frame["edits"].as_array().unwrap().iter().enumerate() {
            actions+=1;attempt(&mut failures,&format!("{}/{}",frame["id"],edit["id"]),||{
                let v=&edit["values"];let advanced=v[5].as_i64().unwrap()!=0;
                let p=openvector::settings::resolve_parameters(&if advanced {json!({"stage":"palette","mode":"advanced","colorSensitivity":(["high","medium","low"][v[4].as_u64().unwrap() as usize]),"antiAlias":v[6].as_i64().unwrap()!=0})}else{json!({"stage":"palette","mode":"basic","category":(["photograph","artwork-with-blending","artwork-without-blending"][v[2].as_u64().unwrap() as usize]),"quality":(["high","medium","low"][v[3].as_u64().unwrap() as usize])})}).unwrap();
                let mode=p["Preprocessor::filter_type"].as_f64().unwrap() as u32;let amount=p["Preprocessor::reduce_noise_order"].as_f64().unwrap() as u32;let anti=p["Shared::is_anti_aliased"].as_f64().unwrap() as u32;let e=&frame["states"][i];
                assert_eq!(json!([mode,amount]),e["effectivePreprocessing"]);let shared=hex(e["sharedBits"].as_str().unwrap());let word=|offset|u32::from_le_bytes(shared[offset..offset+4].try_into().unwrap());assert_eq!(word(0),0);assert_eq!(word(4),anti);assert_eq!(word(20),p["Shared::image_type_code"].as_f64().unwrap() as u32);
                let mut controls=Vec::new();controls.extend_from_slice(&(p["PaletteFinder::cost_per_color"].as_f64().unwrap() as f32).to_le_bytes());for key in ["PaletteFinder::use_16_hist_bins","PaletteFinder::use_26_connected_peaks"]{controls.extend_from_slice(&(p[key].as_f64().unwrap() as u32).to_le_bytes());}assert_eq!(controls,hex(e["paletteControlBits"].as_str().unwrap()));
                let color:[f64;4]=std::array::from_fn(|i|(edit["background"][i].as_f64().unwrap() as f32) as f64);let color_before=color;
                let composed=openvector::image_preparation::composite_source_image(width,height,&original.bgra,matches!(v[7].as_i64().unwrap(),1|2),&color);
                let source=openvector::image_preparation::filter_prepared_image(composed,mode,amount).unwrap();let working=openvector::image_resize::reduce_working_image(source.width,source.height,&source.bgra,anti!=0,true);
                for (kind,image) in [("source",&source),("working",&working)]{assert_eq!(json!([image.width,image.height]),e[format!("{kind}Dimensions")]);assert_eq!(image.bgra,common::b64(e[format!("{kind}Bgra")].as_str().unwrap()));}
                assert_ne!(source.bgra.as_ptr(),original.bgra.as_ptr());assert_eq!(original.bgra,original_before);assert_eq!(bgra,before);assert_eq!(color,color_before);
                assert_eq!(json!([e["status"],e["progress"],e["cancelled"],e["processed"],e["matte"]]),json!([0,100,false,mode!=0,1]));
            });
        }
    }assert_eq!(actions,160);finish(failures);
}

#[test]
fn document_stage_parameter_profiles() {
    let rows=common::read_ndjson("document-segmentation-reference.ndjson.gz");let mut actions=0;let mut failures=Vec::new();
    assert_eq!(rows[0]["wholeNativeResponseRepeatIdentical"],true);
    for frame in &rows[1..] {for (i,step) in frame["steps"].as_array().unwrap().iter().enumerate() {
        actions+=1;attempt(&mut failures,&format!("{}/{}",frame["id"],step["id"]),||{
            let stage=if step["action"]==0{"palette"}else{"segmentation"};let levels=["high","medium","low"];
            let mut input=json!({"stage":stage,"mode":if step["advanced"]==true{"advanced"}else{"basic"}});
            let options=if step["advanced"]!=true {json!({"category":(["photograph","artwork-with-blending","artwork-without-blending"][step["category"].as_u64().unwrap() as usize]),"quality":levels[step["quality"].as_u64().unwrap() as usize]})}
            else if stage=="palette" {json!({"colorSensitivity":levels[step["sensitivity"].as_u64().unwrap() as usize],"antiAlias":step["antiAlias"]})}
            else {json!({"complexity":step["complexity"],"minimumPixels":step["minimumPixels"],"artifactRejection":(["none","moderate","aggressive"][step["artifactRejection"].as_u64().unwrap() as usize]),"clusterColors":step["clusterColors"]})};
            input.as_object_mut().unwrap().extend(options.as_object().unwrap().clone());
            parameters(&openvector::settings::resolve_parameters(&input).unwrap(),&frame["states"][i]["state"]["parameterBits"],&rows[0]["parameters"]);
        });
    }}assert_eq!((rows.len()-1,actions,actions*100),(4,145,14500));finish(failures);
}

#[test]
fn automatic_validation_and_owned_observations() {
    let rows=common::read_ndjson("document-automatic-reference.ndjson.gz");
    let clean=rows.iter().find(|r|r["id"]=="native-gui-clean-session").unwrap();
    let encoded=common::b64(clean["inputs"]["ambivalence-gui-square.bmp"].as_str().unwrap());
    let create=||{let mut d=CoreSession::default().create_document();load_encoded(&mut d,&encoded,"square.bmp");d};
    let mut document=create();let initial=document.snapshot().to_json();
    for controls in [json!({}),json!({"extension":".bmp","quality":9})] {assert!(document.run_automatic(&controls).is_err());compare_json(&document.snapshot().to_json(),&initial,"automatic validation");}
    let baseline=create().run_automatic(&json!({"extension":".bmp"})).unwrap();
    let mut events=Vec::new();let mut progress=|event:&openvector::image_document::DocumentProgress,_state:&DocumentSnapshot| {events.push(event.stage.to_string());Ok(())};
    let mut stage=|_:&str,task:&openvector::image_document::TaskResult,selection:&openvector::image_document::AutomaticSelection| {let mut snapshot=task.state.clone();damage_snapshot(&mut snapshot);let mut selected=selection.clone();selected.settings.palette.fill(f32::NAN);Ok(())};
    let mut result=document.run_automatic_with_observers(&json!({"extension":".bmp"}),Some(&mut progress),Some(&mut stage)).unwrap();
    for stage in ["segmentation","smoothing","fitting"]{assert!(events.iter().any(|e|e==stage));}
    compare_json(&result.state.to_json(),&baseline.state.to_json(),"automatic observer parity");
    compare_json(&Value::Object(result.settings.to_json()),&Value::Object(baseline.settings.to_json()),"automatic observer selection");
    assert_eq!((result.palette_count,result.source_alpha_type,result.recommended_background_mode),(baseline.palette_count,baseline.source_alpha_type,baseline.recommended_background_mode));
    assert!(!result.settings.advanced);let e=&clean["steps"].as_array().unwrap().iter().find(|s|s["action"]=="auto").unwrap()["expected"];
    assert_eq!(result.palette_count,Some(i32_at(e["selectedSettingsBits"].as_str().unwrap(),8) as usize));
    compare_json(&result.state.fitting.as_ref().unwrap().geometry.to_json(),&e["state"]["fittedGeometry"],"clean native fit");assert_eq!(json!(result.state.random_state),e["state"]["randomState"]);
    result.settings.palette.fill(0.0);result.state.original.bgra.fill(0);assert_ne!(document.snapshot().original.bgra,result.state.original.bgra);
}

#[test]
fn batch_validation_export_and_source_alpha_replacement() {
    let rows=common::read_ndjson("batch-image-reference.ndjson.gz");let source=&rows[1]["sources"][0];let mut document=CoreSession::default().create_document();
    let extension=load_encoded(&mut document,&common::b64(source["bytes"].as_str().unwrap()),source["name"].as_str().unwrap());let initial=document.snapshot().to_json();
    for controls in [Value::Null,json!({}),json!({"extension":".png","category":-1}),json!({"extension":".png","quality":4}),json!({"extension":".png","paletteMode":4}),json!({"extension":".png","colorCount":1}),json!({"extension":".png","colorCount":13}),json!({"extension":".png","backgroundMode":2}),json!({"extension":".png","advanced":1})] {
        assert!(document.run_batch_image(&controls).is_err());compare_json(&document.snapshot().to_json(),&initial,"batch validation");
    }
    assert!(document.run_export(|_|Ok(0)).is_err());compare_json(&document.snapshot().to_json(),&initial,"unfitted export");
    let fitted=document.run_batch_image(&json!({"extension":extension})).unwrap();
    let mut input=openvector::vector_output::VectorOutputInput::new(&fitted.state.fitting.as_ref().unwrap().geometry,fitted.state.original.width as i64,fitted.state.original.height as i64);input.layout=openvector::export_options::Layout::GroupedCutouts;
    let directory=tempfile::tempdir().unwrap();let filename=directory.path().join("result.svg");
    let mut exported=document.run_export(|progress|{
        let saved=openvector::vector_file::write_vector_file(&filename,&openvector::vector_file::VectorFileInput::new(input),&mut |percent|progress(percent).map_err(openvector::vector_output::VectorOutputError)).map_err(|error|error.to_string())?;
        assert_eq!(saved.status,0);Ok(0)
    }).unwrap();
    assert_eq!(std::fs::read(&filename).unwrap(),source["expectedSvg"].as_str().unwrap().as_bytes());
    assert_eq!(exported.progress,6);let committed=document.snapshot().to_json();damage_snapshot(&mut exported);compare_json(&document.snapshot().to_json(),&committed,"export snapshot ownership");
    let frame=rows.iter().find(|r|r["id"]=="alpha-auto-background").unwrap();let source=&frame["sources"][0];
    let mut document=CoreSession::default().create_document();let extension=load_encoded(&mut document,&common::b64(source["bytes"].as_str().unwrap()),source["name"].as_str().unwrap());
    let mut controls=frame["controls"].clone();controls["extension"]=json!(extension);
    let actual=document.run_batch_image(&controls).unwrap();editor_support::compare_document_computation(&actual.state,&frame["expectedDocument"]["state"],&rows[0]["parameters"]);assert_eq!(actual.source_alpha_type,2);assert_eq!(actual.settings.background_mode,0);
    let decoded=openvector::image_input::decode_image(&common::b64(source["bytes"].as_str().unwrap())).unwrap();let mut bgra=decoded.bgra;for pixel in bgra.chunks_exact_mut(4){pixel[3]=255}
    let filename=openvector::image_input::resolve_image_filename(source["name"].as_str().unwrap());document.set_source(decoded.width,decoded.height,&bgra,decoded.has_alpha,filename.shadow_control as f64).unwrap();
    assert_eq!(document.run_batch_image(&controls).unwrap().source_alpha_type,0);
}
