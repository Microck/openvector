mod common;
use openvector::{palette::CoreSession,image_document::{ImageDocument,CANCEL_TASK},document_controls::DocumentSettings};
use serde_json::{json,Value};
use std::sync::atomic::Ordering;

fn palette_document()->(ImageDocument,Value){
    let mut document=CoreSession::default().create_document();
    document.set_source(2,2,&[0,0,0,255,255,255,255,255,0,0,0,255,255,255,255,255],true,0.0).unwrap();
    let mut input=Value::Object(DocumentSettings{advanced:true,anti_alias:false,category:2,quality:0,..Default::default()}.to_json());
    input["lower"]=json!(2);input["upper"]=json!(2);(document,input)
}

#[test]
fn cancellation_sentinel_matches_a_checkpoint_cancellation_request(){
    let (mut signalled,input)=palette_document();
    let (mut requested,_)=palette_document();
    let handle=requested.cancellation_handle();
    let signalled_error=signalled.run_task("palette",&input,Some(&mut|_,_|Err(CANCEL_TASK.into()))).unwrap_err();
    let requested_error=requested.run_task("palette",&input,Some(&mut|_,_|{handle.store(true,Ordering::Relaxed);Ok(())})).unwrap_err();
    assert_eq!(signalled_error,requested_error);
    assert_eq!(signalled.snapshot().to_json(),requested.snapshot().to_json());
    assert!(signalled.snapshot().cancelled);
    // Palette interruption propagates before completion; do not manufacture
    // the status=2/progress=100 completion used by caught vector-stage signals.
    assert_eq!(signalled.state().status,0);
    assert_eq!(signalled.state().progress,0);
}

#[test]
fn ordinary_observer_errors_do_not_request_cancellation(){
    let (mut document,input)=palette_document();
    assert_eq!(document.run_task("palette",&input,Some(&mut|_,_|Err("observer failed".into()))).unwrap_err(),"observer failed");
    assert!(!document.snapshot().cancelled);
}

#[test]
fn progress_reports_current_metadata_and_retained_clones_are_independent(){
    let session=CoreSession::default();
    let mut document=session.create_document();
    let pixels=[0,0,0,255,255,255,255,255,0,0,0,255,255,255,255,255];
    document.set_source(2,2,&pixels,true,0.0).unwrap();
    let (_,mut input)=palette_document();
    input["lower"]=json!(4);input["upper"]=json!(4);
    let cancel=document.cancellation_handle();
    let mut retained=None;
    document.run_task("palette",&input,Some(&mut|event,state|{
        assert_eq!(state.random_state,session.random_state());
        assert_eq!(state.cancelled,cancel.load(Ordering::Relaxed));
        assert_eq!(state.progress,event.percent);
        retained=Some(state.clone());
        Ok(())
    })).unwrap();
    let mut retained=retained.unwrap();
    retained.original.bgra.fill(0);
    assert_eq!(document.state().original.bgra,pixels);
    assert!(session.random_state()!=1,"four candidates require random colors");
}

#[test]
fn automatic_observer_cancellation_retries_to_native_png_bytes(){
    use openvector::{image_input::{decode_image,resolve_image_filename},bitmap_export::{export_bitmap,BitmapExportInput,BitmapExportOptions,BitmapFormat}};
    let scene=common::read_ndjson("bitmap-scenes-reference.ndjson.gz").into_iter().find(|row|row["id"]=="opaque-four-plugins").unwrap();
    let decoded=decode_image(&common::b64(scene["sourceBytes"].as_str().unwrap())).unwrap();
    let filename=resolve_image_filename(scene["sourceFilename"].as_str().unwrap());
    let mut document=CoreSession::default().create_document();
    document.set_source(decoded.width,decoded.height,&decoded.bgra,decoded.has_alpha,filename.shadow_control as f64).unwrap();
    let input=json!({"extension":"bmp"});
    assert_eq!(document.run_automatic_with_observer(&input,Some(&mut|_,_|Err(CANCEL_TASK.into()))).unwrap_err(),CANCEL_TASK);
    assert!(document.snapshot().cancelled);
    let result=document.run_automatic(&input).unwrap();
    assert!(!result.state.cancelled);
    let fitting=result.state.fitting.as_ref().unwrap();assert!(fitting.complete);
    let background=document.background_state();let hidden:Vec<i64>=background.hidden_regions.iter().map(|v|*v as i64).collect();
    let bitmap=BitmapExportInput{geometry:Some(&fitting.geometry),width:result.state.original.width as i64,height:result.state.original.height as i64,source_alpha_type:result.source_alpha_type,background_mode:result.state.settings.background_mode,background_undo_count:background.undo.len(),hidden_regions:&hidden};
    assert_eq!(export_bitmap(&bitmap,&BitmapExportOptions::new(BitmapFormat::Png)).unwrap(),common::b64(scene["expected"]["result.png"]["bytes"].as_str().unwrap()));
    // The generic host export callback uses the same reserved signal contract.
    assert_eq!(document.run_export(|_|Err(CANCEL_TASK.into())).unwrap_err(),CANCEL_TASK);
    assert!(document.snapshot().cancelled);
}
