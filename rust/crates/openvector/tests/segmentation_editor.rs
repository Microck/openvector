mod common;
mod editor_support;
use serde_json::{Value,json};
use openvector::segmentation_editor::{EditorSnapshot,SegmentationEditor};
use openvector::editor_input::EditorPointer;
use editor_support::{automatic_document,compare_document_state,compare_json,hex};
fn tool(value:&Value)->&'static str{match value.as_i64().unwrap(){0=>"pan",3=>"pencil",4=>"picker",5=>"fill",6=>"zap",_=>panic!("unrecognized native tool")}}
fn command(action:&Value)->Value{let color:Vec<Value>=action["argb"].as_array().unwrap().iter().rev().cloned().collect();if action["kind"]==3{let coordinates=action["coordinates"].as_array().unwrap();json!({"tool":"pencil","from":coordinates[..2],"to":coordinates[2..],"color":color,"commit":action["commit"]})}else{json!({"tool":if action["kind"]==5{"fill"}else{"zap"},"x":action["x"],"y":action["y"],"color":color,"commit":action["commit"]})}}
fn compare_editor(actual:&EditorSnapshot,expected:&Value,label:&str){
    assert_eq!(actual.bgra,hex(expected["bgra"].as_str().unwrap()),"{label} pixels");assert_eq!(actual.key,expected["key"].as_str().unwrap(),"{label} key");
    let actual_json=serde_json::to_value(actual).unwrap();for list in ["undo","redo"]{let expected=Value::Array(expected[list].as_array().unwrap().iter().map(|v|{let mut record=v.clone();record.as_object_mut().unwrap().remove("bits");record}).collect());compare_json(&actual_json[list],&expected,&format!("{label} {list}"));}
    if let Some(mode)=expected.get("inputMode"){assert_eq!(actual.tool,tool(mode),"{label} tool");let color:Vec<Value>=expected["paintArgb"].as_array().unwrap().iter().rev().cloned().collect();compare_json(&json!(actual.color),&json!(color),&format!("{label} paint"));}
}

// Historical alpha refits read beyond the live palette. Require a complete,
// finite graph and exact painted region colors instead of those arbitrary bits.
fn assert_completed_edited_fit(state:&openvector::image_document::DocumentSnapshot,editor:&EditorSnapshot){
    assert_eq!(state.status,0);
    let fitting=state.fitting.as_ref().unwrap();
    assert!(fitting.complete);
    let geometry=&fitting.geometry;
    for node in &geometry.nodes{assert!(node.x.is_finite()&&node.y.is_finite());}
    for cubic in &geometry.cubics{assert!(cubic.points.iter().all(|v|v.is_finite()));}
    let contours=state.contours.as_ref().unwrap();
    for node in &contours.nodes{assert!(node.x.is_finite()&&node.y.is_finite());}
    assert!(state.smoothing.as_ref().unwrap().reference_positions.iter().all(|v|v.is_finite()));
    let map=state.segmentation.as_ref().unwrap();
    assert_eq!(geometry.regions.len(),map.counts.len());
    let mut counts=vec![0;geometry.regions.len()];
    for (pixel,&region) in map.indices.as_ref().unwrap().iter().enumerate(){
        let region=region as usize;
        counts[region]+=1;
        assert_eq!(geometry.regions[region].bgra.as_slice(),&editor.bgra[pixel*4..pixel*4+4],"painted pixel {pixel}");
        assert_eq!(contours.regions[region].bgra,geometry.regions[region].bgra);
    }
    assert_eq!(counts,map.counts);
    assert_eq!(geometry.regions.iter().map(|r|r.pixel_count).collect::<Vec<_>>(),counts);
    let input=openvector::vector_output::VectorOutputInput::new(geometry,editor.width as i64,editor.height as i64);
    let svg=openvector::svg::export_svg(&input).unwrap();
    assert!(!svg.contains("NaN")&&!svg.contains("inf"));
    for region in &geometry.regions{
        let [b,g,r,a]=region.bgra;
        if a>10{assert!(svg.contains(&format!("#{r:02x}{g:02x}{b:02x}")),"serialized painted color {:?}",region.bgra);}
    }
}

fn short_stroke_expected(initial:&EditorSnapshot,frame:usize,native:&Value)->EditorSnapshot{
    use openvector::segmentation_editor::ToolRecord;
    let record=|x,y,coordinates,key:&str|ToolRecord{kind:3,x,y,coordinates,argb:[1.0,0.0,0.0,0.0],key:key.into(),joins_previous:false};
    let records=[
        record(0,8,[0.9375,8.5,0.9375,8.5],"3 0 8 0.9375 8.5 0.9375 8.5 [1, 0, 0, 0]"),
        record(1,8,[1.25,8.5,0.9375,8.5],"3 1 8 1.25 8.5 0.9375 8.5 [1, 0, 0, 0]"),
        record(0,10,[0.9375,10.5,0.9375,10.5],"3 0 10 0.9375 10.5 0.9375 10.5 [1, 0, 0, 0]"),
        record(1,10,[1.0625,10.5,0.9375,10.5],"3 1 10 1.0625 10.5 0.9375 10.5 [1, 0, 0, 0]"),
    ];
    let active=match frame{0..=4=>0,5|8=>1,6|7|9=>2,10=>3,_=>4};
    let mut expected=initial.clone();
    expected.tool=tool(&native["inputMode"]).into();
    expected.color=std::array::from_fn(|c|native["paintArgb"][3-c].as_f64().unwrap() as f32);
    expected.undo=records[..active].to_vec();
    expected.redo=if frame==8{vec![records[1].clone()]}else{Vec::new()};
    expected.key=expected.undo.iter().map(|record|record.key.as_str()).collect();
    for (record,&(x,y)) in [(0,8),(1,8),(0,10),(1,10)].iter().enumerate(){
        if record<active{let offset=(y*initial.width+x)*4;expected.bgra[offset..offset+4].copy_from_slice(&[0,0,0,255]);}
    }
    expected
}
fn run_native_command_workflow(id:&str) {
    static ROWS:std::sync::LazyLock<Vec<Value>>=std::sync::LazyLock::new(||{
        let rows=common::read_ndjson("segmentation-edit-reference.ndjson.gz");
        assert_eq!(rows.len(),12);
        let ids:Vec<&str>=rows[1..].iter().map(|row|row["id"].as_str().unwrap()).collect();
        assert_eq!(ids,vec!["pilot-complete","history-order","color-precision","color-rounding",
            "fill-contact","palette-reorder","pencil-lines","zap-tie","zap-blended","flexible",
            "alpha-premultiplied"]);
        rows
    });
    let metadata=&ROWS[0];
    let workflow=ROWS[1..].iter().find(|row|row["id"]==id).expect("declared native workflow");
    let (mut document,automatic)=automatic_document(&workflow["input"]);
    compare_editor(&document.begin_editing().unwrap(),&workflow["expectedFrames"][0],id);
    let mut completed=0;
    for(index,action)in workflow["actions"].as_array().unwrap().iter().enumerate() {
        let editor=match action["action"].as_i64().unwrap() {
            1=>document.edit(&command(action)).unwrap(),
            2=>document.undo_edit().unwrap(),
            3=>document.redo_edit().unwrap(),
            4=>document.revert_edits().unwrap(),
            5=>{
                let state=document.finish_editing(&Value::Object(automatic.settings.to_json()),None)
                    .unwrap_or_else(|error|panic!("{id}/{index} finish editing: {error}"));
                if id=="alpha-premultiplied"{assert_completed_edited_fit(&state,&document.editing_state().unwrap());}
                else{compare_document_state(&state,&workflow["expectedCompleted"][completed]["state"],&metadata["parameters"]);}
                completed+=1;
                document.begin_editing().unwrap()
            },
            other=>panic!("unknown action {other}")
        };
        compare_editor(&editor,&workflow["expectedFrames"][index+1],&format!("{id}/{index}"));
        compare_json(&json!([editor.width,editor.height]),
            &Value::Array(workflow["expectedFrames"][index+1]["header"].as_array().unwrap()[3..5].to_vec()),
            "editor dimensions");
    }
    assert_eq!(completed,workflow["expectedCompleted"].as_array().unwrap().len());
}

macro_rules! native_command_workflows {
    ($($name:ident => $id:literal),+ $(,)?) => {
        $(#[test] fn $name(){run_native_command_workflow($id);})+
    };
}
native_command_workflows! {
    native_command_pilot_complete => "pilot-complete",
    native_command_history_order => "history-order",
    native_command_color_precision => "color-precision",
    native_command_color_rounding => "color-rounding",
    native_command_fill_contact => "fill-contact",
    native_command_palette_reorder => "palette-reorder",
    native_command_pencil_lines => "pencil-lines",
    native_command_zap_tie => "zap-tie",
    native_command_zap_blended => "zap-blended",
    native_command_flexible => "flexible",
    native_command_alpha_premultiplied => "alpha-premultiplied",
}
#[test]
fn all_16_native_gesture_workflows_and_completed_refits(){
    let rows=common::read_ndjson("segmentation-gesture-reference.ndjson.gz");assert_eq!(rows.len(),17);let metadata=&rows[0];
    for workflow in &rows[1..]{let label=workflow["id"].as_str().unwrap();let (mut document,automatic)=automatic_document(&workflow["input"]);let initial=document.begin_editing().unwrap();assert_eq!(initial.tool,tool(&workflow["initialMode"]));compare_json(&json!(initial.color),&Value::Array(workflow["initialPaintArgb"].as_array().unwrap().iter().rev().cloned().collect()),"initial paint");compare_editor(&initial,&workflow["expectedFrames"][0],label);let mut completed=0;
        for(index,action)in workflow["actions"].as_array().unwrap().iter().enumerate(){
            let editor=if action["action"]==6{
                document.gesture(&json!({"phase":"select","tool":tool(&action["tool"])})).unwrap()
            }else if action.get("phase").is_some(){
                document.gesture(&json!({"phase":action["phase"],"point":action["point"],"buttons":action["buttons"],"button":action["button"],"modifiers":action["modifiers"]})).unwrap()
            }else{match action["action"].as_i64().unwrap(){
                2=>document.undo_edit().unwrap(),3=>document.redo_edit().unwrap(),4=>document.revert_edits().unwrap(),10=>document.begin_editing().unwrap(),
                11=>document.gesture(&json!({"phase":"color","color":action["argb"].as_array().unwrap().iter().rev().collect::<Vec<_>>()})).unwrap(),
                5=>{
                    let state=document.finish_editing(&Value::Object(automatic.settings.to_json()),None).unwrap();
                    if label=="pencil-short-samples"{assert_completed_edited_fit(&state,&document.editing_state().unwrap());}
                    else{compare_document_state(&state,&workflow["expectedCompleted"][completed]["state"],&metadata["parameters"]);}
                    completed+=1;document.begin_editing().unwrap()
                },
                other=>panic!("unknown gesture action {other}")
            }};
            let native=&workflow["expectedFrames"][index+1];
            if label=="pencil-short-samples"{
                assert_eq!(editor,short_stroke_expected(&initial,index+1,native),"{label}/{index}");
                assert_eq!([editor.width,editor.height],[native["header"][3].as_u64().unwrap() as usize,native["header"][4].as_u64().unwrap() as usize]);
            }else{compare_editor(&editor,native,&format!("{label}/{index}"));}
        }
        assert_eq!(completed,workflow["expectedCompleted"].as_array().unwrap().len());
    }
}
#[test]
fn native_initial_pointer_and_outside_anchor_behavior(){
    let rows=common::read_ndjson("editor-pointer-reference.ndjson.gz");assert_eq!(rows.iter().map(|v|v["stage"].as_str().unwrap()).collect::<Vec<_>>(),vec!["cold","first-source","second-source"]);for row in &rows{compare_json(&json!(EditorPointer::default().previous_point),&row["pointer"]["previous"],"native pointer");}
    let pixels=vec![255;12*12*4];let mut editor=SegmentationEditor::new(12,12,&pixels,&pixels,&[0.0,0.0,0.0,1.0],&[0.0,0.0,1.0,1.0],0).unwrap();let mut pointer=EditorPointer::default();editor.gesture(&json!({"phase":"select","tool":"pencil"}),&mut pointer).unwrap();let first=editor.gesture(&json!({"phase":"move","point":[3,3],"buttons":1}),&mut pointer).unwrap();assert_eq!(first.bgra,pixels);assert!(first.undo.is_empty());editor.gesture(&json!({"phase":"move","point":[10,9],"buttons":0}),&mut pointer).unwrap();let second=editor.gesture(&json!({"phase":"move","point":[5,3],"buttons":1}),&mut pointer).unwrap();let mut expected=pixels.clone();for x in 3..=5{expected[(3*12+x)*4..(3*12+x)*4+4].copy_from_slice(&[0,0,255,255]);}assert_eq!(second.bgra,expected);assert_eq!(second.undo.len(),1);assert_eq!(second.undo[0].coordinates,[5.0,3.0,3.0,3.0]);assert_eq!(editor.revert().unwrap().bgra,pixels);
}
#[test]
fn editor_validation_checkpoint_ownership_and_noncommitted_replay(){
    let pixels=vec![255;16];let mut editor=SegmentationEditor::new(2,2,&pixels,&pixels,&[0.0,0.0,0.0,1.0],&[0.0,0.0,0.0,1.0],0).unwrap();let before=editor.snapshot();for invalid in [json!({"tool":"pencil","from":[0,0],"to":[1,null],"color":[0,0,0,1]}),json!({"tool":"fill","x":0,"y":0,"color":[0,0,0,2]}),json!({"tool":"fill","x":0,"y":0,"color":[0,0,0,1],"commit":1})]{assert!(editor.apply(&invalid).is_err());assert_eq!(editor.snapshot(),before);}
    let painted=editor.apply(&json!({"tool":"pencil","from":[0,0],"to":[0,0],"color":[0,0,1,1]})).unwrap();let checkpoint=editor.history_checkpoint();let mut leaked=painted.clone();leaked.bgra.fill(7);leaked.undo[0].argb.fill(0.0);assert_eq!(editor.snapshot(),painted);
    editor.apply(&json!({"tool":"fill","x":1,"y":1,"color":[0,1,0,1],"commit":false})).unwrap();assert_eq!(editor.history_state(),(1,0));assert_eq!(editor.restore_checkpoint(&checkpoint).unwrap(),painted);assert_eq!(editor.undo().unwrap().bgra,pixels);assert_eq!(editor.redo().unwrap(),painted);
}

#[test]
fn back_checkpoint_compares_active_serialized_records_only(){
    let pixels=vec![255;16];
    let mut editor=SegmentationEditor::new(2,2,&pixels,&pixels,&[0.0,0.0,0.0,1.0],&[0.0,0.0,0.0,1.0],0).unwrap();
    editor.apply(&json!({"tool":"pencil","from":[0.12345671,0],"to":[0.12345671,0],"color":[0,0,0,1]})).unwrap();
    let first=editor.history_checkpoint();
    editor.undo().unwrap();
    assert!(editor.matches_active_checkpoint(None),"empty active history ignores redo");
    editor.apply(&json!({"tool":"pencil","from":[0.12345672,0],"to":[0.12345672,0],"color":[0,0,0,1]})).unwrap();
    assert_ne!(editor.history_checkpoint().undo,first.undo,"raw coordinates differ");
    assert!(editor.matches_active_checkpoint(Some(&first)),"canonical six-digit records agree");
    editor.apply(&json!({"tool":"pencil","from":[1,1],"to":[1,1],"color":[0,0,0,1]})).unwrap();
    let second=editor.history_checkpoint();
    assert!(!editor.matches_active_checkpoint(Some(&first)),"proper prefix differs");
    editor.undo().unwrap();
    assert!(editor.matches_active_checkpoint(Some(&first)),"redo is ignored");
    assert!(!editor.matches_active_checkpoint(Some(&second)),"shorter active list differs");
    editor.restore_checkpoint(&first).unwrap();
    assert_eq!(editor.history_state(),(1,0),"restore copies redo too");
}

#[test]
fn applied_edits_preserve_back_checkpoint_and_explicit_commit(){
    let rows=common::read_ndjson("segmentation-edit-reference.ndjson.gz");
    let (mut document,_)=automatic_document(&rows[1]["input"]);
    let baseline=document.begin_editing().unwrap();
    let edited=document.edit(&command(&rows[1]["actions"][0])).unwrap();
    document.apply_editing().unwrap();
    assert!(document.editing_changed());
    assert!(document.has_applied_edits());
    assert!(!document.has_committed_edits());
    let applied=document.state().stage_keys.segmentation_edits.clone();
    let restored=document.restore_committed_editing().unwrap().unwrap();
    assert_eq!(restored.bgra,baseline.bgra);
    assert_eq!((restored.undo.len(),restored.redo.len()),(0,0));
    assert_eq!(document.state().stage_keys.segmentation_edits,applied);
    document.edit(&command(&rows[1]["actions"][0])).unwrap();
    document.commit_editing().unwrap();
    assert!(!document.editing_changed());
    document.undo_edit().unwrap();
    assert!(document.editing_changed());
    assert_eq!(document.restore_committed_editing().unwrap().unwrap().bgra,edited.bgra);
    document.reset();
    assert!(!document.has_applied_edits());
    assert!(!document.has_committed_edits());
    assert!(!document.editing_changed());
}

#[test]
fn successful_smoothing_checkpoint_survives_fitting_cancel_and_cache_skips(){
    use openvector::image_document::CANCEL_TASK;
    let rows=common::read_ndjson("segmentation-edit-reference.ndjson.gz");
    let (mut document,automatic)=automatic_document(&rows[1]["input"]);
    let controls=Value::Object(automatic.settings.to_json());
    let baseline=document.begin_editing().unwrap();
    let edited=document.edit(&command(&rows[1]["actions"][0])).unwrap();
    document.apply_editing().unwrap();
    let cancelled=document.smooth(&controls,Some(&mut|_,_|Err(CANCEL_TASK.into()))).unwrap();
    assert_eq!(cancelled.status,2);
    assert!(document.editing_changed());
    assert_eq!(document.restore_committed_editing().unwrap().unwrap().bgra,baseline.bgra);
    document.edit(&command(&rows[1]["actions"][0])).unwrap();
    document.apply_editing().unwrap();
    assert_eq!(document.smooth(&controls,None).unwrap().status,0);
    assert!(!document.editing_changed());
    assert_eq!(document.fit(&controls,Some(&mut|_,_|Err(CANCEL_TASK.into()))).unwrap().status,2);
    document.undo_edit().unwrap();
    assert_eq!(document.restore_committed_editing().unwrap().unwrap().bgra,edited.bgra);
    document.fit(&controls,None).unwrap();
    document.undo_edit().unwrap();
    for stage in ["segmentation","smoothing","fitting"]{
        assert!(!document.run_task(stage,&controls,None).unwrap().did_work);
        assert!(document.editing_changed(),"scheduler {stage} skip must not save history");
    }
    // Direct cached methods do copy histories, unlike the scheduler skip.
    document.smooth(&controls,None).unwrap();
    assert!(!document.editing_changed());
    assert_eq!(document.editing_history(),(0,1));
    document.redo_edit().unwrap();
    document.fit(&controls,None).unwrap();
    assert!(!document.editing_changed());
    document.undo_edit().unwrap();
    assert_eq!(document.restore_committed_editing().unwrap().unwrap().bgra,edited.bgra);
}

#[test]
fn classification_saves_histories_but_control_preload_does_not(){
    let rows=common::read_ndjson("segmentation-edit-reference.ndjson.gz");
    let (mut document,automatic)=automatic_document(&rows[1]["input"]);
    document.begin_editing().unwrap();
    document.edit(&command(&rows[1]["actions"][0])).unwrap();
    document.edit(&command(&rows[1]["actions"][5])).unwrap();
    document.undo_edit().unwrap();
    let selected=document.editing_state().unwrap();
    assert_eq!((selected.undo.len(),selected.redo.len()),(1,1));
    let mut controls=Value::Object(automatic.settings.to_json());
    document.commit_settings(&controls).unwrap();
    assert!(document.editing_changed(),"control preload must not save editor history");
    assert!(document.classify(&controls).is_err(),"missing extension rejects before saving");
    assert!(document.editing_changed());
    controls["extension"]=json!("bmp");
    let classified=document.classify(&controls).unwrap();
    assert_eq!(classified.state.status,0);
    assert!(!document.editing_changed());
    document.redo_edit().unwrap();
    assert_eq!(document.restore_committed_editing().unwrap().unwrap(),selected);
}
#[test]
fn alpha_edit_creation_palette_native_bits(){
    let reference=common::read_json("edited-alpha-palette-reference.json");let rows=common::read_ndjson("segmentation-edit-reference.ndjson.gz");let workflow=rows.iter().find(|v|v["id"]=="alpha-premultiplied").unwrap();assert_eq!(workflow["input"]["sha256"],reference["inputSha256"]);let (mut document,_)=automatic_document(&workflow["input"]);document.begin_editing().unwrap();let actions=workflow["actions"].as_array().unwrap();for action in &actions[..actions.len()-1]{match action["action"].as_i64().unwrap(){1=>{document.edit(&command(action)).unwrap();},2=>{document.undo_edit().unwrap();},3=>{document.redo_edit().unwrap();},4=>{document.revert_edits().unwrap();},other=>panic!("unexpected alpha action {other}")}}assert_eq!(actions.last().unwrap()["action"],5);let state=document.commit_editing().unwrap();let palette=&state.segmentation.as_ref().unwrap().palette;assert_eq!(palette.len()/4,3);assert_eq!(palette.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>(),hex(reference["creationPaletteBgraBits"].as_str().unwrap()));
}

#[test]
fn native_alpha_edit_stable_properties_before_refit(){
    let stable=common::read_json("alpha-nondeterminism-20261020.json");
    let rows=common::read_ndjson("segmentation-edit-reference.ndjson.gz");
    let workflow=rows.iter().find(|row|row["id"]==stable["scenario"]).unwrap();
    assert_eq!(workflow["input"]["sha256"],stable["inputSha256"]);
    assert_eq!(stable["nativeResponseSha256"].as_array().unwrap().len(),3);
    let (mut document,automatic)=automatic_document(&workflow["input"]);
    compare_document_state(&automatic.state,&workflow["expectedAutomatic"]["state"],&rows[0]["parameters"]);
    assert_eq!(automatic.state.original.bgra,hex(stable["originalBgra"].as_str().unwrap()),"loaded original pixels");
    compare_editor(&document.begin_editing().unwrap(),&workflow["expectedFrames"][0],"alpha initial");

    let actions=workflow["actions"].as_array().unwrap();
    assert_eq!(actions.len(),8);
    assert_eq!(actions.last().unwrap()["action"],5);
    for(index,action)in actions[..actions.len()-1].iter().enumerate(){
        let editor=match action["action"].as_i64().unwrap(){
            1=>document.edit(&command(action)).unwrap(),
            2=>document.undo_edit().unwrap(),
            3=>document.redo_edit().unwrap(),
            4=>document.revert_edits().unwrap(),
            other=>panic!("unexpected pre-refit alpha action {other}"),
        };
        let frame=&workflow["expectedFrames"][index+1];
        compare_editor(&editor,frame,&format!("alpha stable action {index}"));
        assert_eq!(document.snapshot().random_state as u64,frame["header"][7].as_u64().unwrap(),"editor random state");
    }

    // Native pre-refit state is well-defined. The complete workflow separately
    // checks finite refitting and serialization against painted-color invariants.
    let state=document.commit_editing().unwrap();
    compare_editor(&document.editing_state().unwrap(),workflow["expectedFrames"].as_array().unwrap().last().unwrap(),"alpha committed editor");
    let snapshot=state.to_json();
    for(field,expected)in stable["segmentation"].as_object().unwrap(){
        compare_json(&snapshot["segmentation"][field],expected,&format!("committed segmentation {field}"));
    }
    let map=state.segmentation.as_ref().unwrap();
    let region_colors:Vec<[u8;4]>=map.color_indices.iter().map(|&index|{
        std::array::from_fn(|channel|(map.palette[index as usize*4+channel] as f64*255.0+0.5).floor() as u8)
    }).collect();
    compare_json(&json!(region_colors),&stable["regionBgra"],"committed region creation colors");
    editor_support::compare_settings(&state.settings,stable["settingsBits"].as_str().unwrap(),stable["committedPaletteArgbBits"].as_str().unwrap());
    assert_eq!(state.random_state as u64,stable["preRefitRandomState"].as_u64().unwrap(),"commit does not consume random draws");
    assert_eq!(state.original.bgra,hex(stable["originalBgra"].as_str().unwrap()),"unchanged original pixels");
    for(kind,image)in [("source",&state.source),("working",&state.working)]{
        compare_json(&json!([image.width,image.height]),&stable[format!("{kind}Dimensions")],&format!("{kind} dimensions"));
        assert_eq!(image.bgra,common::b64(stable[format!("{kind}Bgra")].as_str().unwrap()),"{kind} pixels");
    }
    for(name,descriptor)in rows[0]["parameters"].as_object().unwrap(){
        let value=&state.parameters[name];
        let values:Vec<&Value>=if let Some(values)=value.as_array(){values.iter().collect()}else{vec![value]};
        assert_eq!(values.len(),descriptor["count"].as_u64().unwrap() as usize,"{name} count");
        let mut bits=Vec::new();
        for value in values{
            let number=value.as_f64().unwrap();
            match descriptor["kind"].as_str().unwrap(){
                "float64"=>bits.extend_from_slice(&number.to_le_bytes()),
                "float32"=>bits.extend_from_slice(&(number as f32).to_le_bytes()),
                "int32"=>bits.extend_from_slice(&(number as i32).to_le_bytes()),
                other=>panic!("unexpected native parameter kind {other}"),
            }
        }
        assert_eq!(bits,hex(stable["parameterBits"][name].as_str().unwrap()),"{name} bits");
    }
}

#[test]
fn alpha_edit_sampling_slots_survive_cancellation_retry_and_cached_finish(){
    use openvector::image_document::CANCEL_TASK;
    let rows=common::read_ndjson("segmentation-edit-reference.ndjson.gz");
    let workflow=rows.iter().find(|row|row["id"]=="alpha-premultiplied").unwrap();
    for target in ["smoothing","fitting"]{
        let (mut document,automatic)=automatic_document(&workflow["input"]);
        let controls=Value::Object(automatic.settings.to_json());
        document.begin_editing().unwrap();
        for action in workflow["actions"].as_array().unwrap().iter().take(7){
            match action["action"].as_i64().unwrap(){
                1=>{document.edit(&command(action)).unwrap();},
                2=>{document.undo_edit().unwrap();},
                3=>{document.redo_edit().unwrap();},
                4=>{document.revert_edits().unwrap();},
                other=>panic!("unexpected alpha action {other}")
            }
        }
        let edited=document.editing_state().unwrap();
        assert_eq!(document.commit_editing().unwrap().segmentation.unwrap().palette.len(),12);
        assert_eq!(automatic.settings.palette.len(),8);
        let result=document.finish_editing(&controls,Some(&mut|event,_|{
            if event.stage==target{Err(CANCEL_TASK.into())}else{Ok(())}
        }));
        assert_eq!(result.unwrap_err(),CANCEL_TASK);
        assert_eq!(document.state().status,2);
        assert!(document.state().stage_keys.fitting.is_empty());
        assert_eq!(document.editing_history(),(edited.undo.len(),edited.redo.len()));
        document.undo_edit().unwrap();
        assert_eq!(document.restore_committed_editing().unwrap().unwrap().bgra,edited.bgra);
        let complete=document.finish_editing(&controls,None).unwrap();
        assert_completed_edited_fit(&complete,&edited);
        assert_eq!(complete.segmentation.as_ref().unwrap().palette,automatic.settings.palette,"live controls remain separate from owned creation slots");
        assert_eq!(document.finish_editing(&controls,None).unwrap().to_json(),complete.to_json(),"cached finish retains geometry, palette and random stream");
    }
}
