#[path="../../openvector/tests/common/mod.rs"] mod common;
use serde_json::{json,Value};
use std::{path::{Path,PathBuf},process::{Command,Output,Stdio},io::Write,sync::atomic::{AtomicU64,Ordering}};
struct Directory(PathBuf);
impl Directory{fn new()->Self{static NEXT:AtomicU64=AtomicU64::new(0);let p=std::env::temp_dir().join(format!("openvector-cli-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir(&p).unwrap();Self(p)}}
impl Drop for Directory{fn drop(&mut self){std::fs::remove_dir_all(&self.0).unwrap();}}
fn invoke_with_input_result(root:&Path,args:&[&str],stdin:Option<&[u8]>)->(Output,std::io::Result<()>){
    let mut child=Command::new(env!("CARGO_BIN_EXE_openvector")).current_dir(root).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let written=if let Some(input)=stdin{child.stdin.take().unwrap().write_all(input)}else{drop(child.stdin.take());Ok(())};
    (child.wait_with_output().unwrap(),written)
}
fn invoke(root:&Path,args:&[&str],stdin:Option<&[u8]>)->Output{
    let (output,written)=invoke_with_input_result(root,args,stdin);
    written.unwrap_or_else(|error|panic!("{args:?}: stdin write failed: {error}; exit {:?}; stderr {}",output.status.code(),String::from_utf8_lossy(&output.stderr)));
    output
}
fn reject_stdin(root:&Path,args:&[&str],stdin:&[u8],diagnostic:&str){
    let (output,written)=invoke_with_input_result(root,args,Some(stdin));
    failure(&output,diagnostic);
    // Admission can reject before reading stdin, closing the consumer's pipe.
    if let Err(error)=written{assert_eq!(error.kind(),std::io::ErrorKind::BrokenPipe,"{args:?}: {error}");}
}
fn success(output:&Output){assert_eq!(output.status.code(),Some(0),"{}",String::from_utf8_lossy(&output.stderr));assert!(output.stderr.is_empty(),"{}",String::from_utf8_lossy(&output.stderr));}
fn failure(output:&Output,diagnostic:&str){assert_eq!(output.status.code(),Some(1));assert!(output.stdout.is_empty());assert!(String::from_utf8_lossy(&output.stderr).contains(diagnostic),"{}",String::from_utf8_lossy(&output.stderr));}
fn automatic()->(Vec<u8>,Value){let row=common::read_ndjson("document-automatic-reference.ndjson.gz").into_iter().find(|r|r["id"]=="native-gui-clean-session").unwrap();let bytes=common::b64(row["inputs"]["ambivalence-gui-square.bmp"].as_str().unwrap());let state=row["steps"].as_array().unwrap().iter().find(|r|r["action"]=="auto").unwrap()["expected"]["state"].clone();(bytes,state)}
fn svg(state:&Value,hidden:&[i64])->Vec<u8>{let geometry=openvector::contour_fitting::FittedGeometry::from_json(&state["fittedGeometry"]).unwrap();let mut input=openvector::vector_output::VectorOutputInput::new(&geometry,state["segmentation"]["width"].as_i64().unwrap(),state["segmentation"]["height"].as_i64().unwrap());input.hidden_regions=hidden;input.layout=openvector::export_options::Layout::GroupedCutouts;openvector::svg::export_svg(&input).unwrap().into_bytes()}
#[test]
fn vectorize_native_geometry_stdin_output_protection_and_bitmaps(){
    let directory=Directory::new();let root=&directory.0;let (bytes,state)=automatic();std::fs::write(root.join("input image.bmp"),&bytes).unwrap();let expected=svg(&state,&[]);
    let output=invoke(root,&["vectorize","input image.bmp"],None);success(&output);assert_eq!(output.stdout,expected);
    let output=invoke(root,&["vectorize","-","--filename","square.bmp"],Some(&bytes));success(&output);assert_eq!(output.stdout,expected);
    reject_stdin(root,&["vectorize","-"],&bytes,"--filename");failure(&invoke(root,&["vectorize","input image.bmp","--filename","square.bmp"],None),"only valid with stdin");
    success(&invoke(root,&["vectorize","-o","result.SVG","input image.bmp"],None));assert_eq!(std::fs::read(root.join("result.SVG")).unwrap(),expected);
    failure(&invoke(root,&["vectorize","input image.bmp","-o","result.SVG"],None),"--force");assert_eq!(std::fs::read(root.join("result.SVG")).unwrap(),expected);
    success(&invoke(root,&["vectorize","input image.bmp","-o","result.SVG","--force"],None));
    std::fs::hard_link(root.join("input image.bmp"),root.join("hardlink.svg")).unwrap();
    #[cfg(unix)] std::os::unix::fs::symlink("input image.bmp",root.join("symlink.svg")).unwrap();
    #[cfg(unix)] let aliases=["input image.bmp","hardlink.svg","symlink.svg"];
    #[cfg(not(unix))] let aliases=["input image.bmp","hardlink.svg"];
    for alias in aliases{
        failure(&invoke(root,&["vectorize","input image.bmp","-o",alias,"--format","svg","--force"],None),"different files");
        assert_eq!(std::fs::read(root.join("input image.bmp")).unwrap(),bytes);
    }
    // Compare actual files, not lowercased names on a case-sensitive volume.
    let aliases_source=root.join("INPUT IMAGE.BMP").exists();
    let output=invoke(root,&["vectorize","input image.bmp","-o","INPUT IMAGE.BMP","--format","svg","--force"],None);
    if aliases_source{
        failure(&output,"different files");
    }else{
        success(&output);assert_eq!(std::fs::read(root.join("INPUT IMAGE.BMP")).unwrap(),expected);
    }
    assert_eq!(std::fs::read(root.join("input image.bmp")).unwrap(),bytes);
    let geometry=openvector::contour_fitting::FittedGeometry::from_json(&state["fittedGeometry"]).unwrap();
    let mut vector=openvector::vector_output::VectorOutputInput::new(&geometry,16,16);vector.layout=openvector::export_options::Layout::GroupedCutouts;
    let palette=state["segmentation"]["palette"].as_array().unwrap().iter().map(|v|v.as_f64().unwrap() as f32).collect::<Vec<_>>();
    for format in ["svg","pdf","ai","eps","dxf"]{
        let output=invoke(root,&["vectorize","input image.bmp","--format",format],None);success(&output);
        let expected=match format{
            "svg"=>openvector::svg::export_svg(&vector).unwrap().into_bytes(),
            "pdf"=>openvector::pdf::export_pdf(&vector).unwrap(),
            "ai"=>openvector::ai::export_ai(&vector).unwrap().into_bytes(),
            "eps"=>{let text=std::str::from_utf8(&output.stdout).unwrap();let dates=text.lines().filter_map(|line|line.strip_prefix("%%CreationDate: ")).collect::<Vec<_>>();assert_eq!(dates.len(),1);openvector::eps::export_eps(&vector,&openvector::eps::EpsOptions{compact:false,creation_date:Some(dates[0].into())}).unwrap().into_bytes()},
            _=>openvector::dxf::export_dxf(&vector,&palette,openvector::dxf::DxfCurveMode::DenseLines).unwrap().into_bytes()
        };assert_eq!(output.stdout,expected,"{format}");
        let mut request=json!({"geometry":state["fittedGeometry"],"width":16,"height":16,"palette":palette,"layout":"grouped-cutouts"});
        if format=="eps"{request["creationDate"]=json!(std::str::from_utf8(&expected).unwrap().lines().find_map(|line|line.strip_prefix("%%CreationDate: ")).unwrap());}
        let low_level=invoke(root,&[&format!("export-{format}"),"-"],Some(&serde_json::to_vec(&request).unwrap()));success(&low_level);
        assert_eq!(low_level.stdout,expected,"low-level {format}");
    }
    for format in ["bmp","png","jpeg","tiff"]{let output=invoke(root,&["vectorize","input image.bmp","--format",format,"--scale","2"],None);success(&output);let image=openvector::image_input::decode_image(&output.stdout).unwrap();assert_eq!((image.width,image.height),(32,32));}
    for (args,error) in [(vec!["vectorize","input image.bmp","--scale","2"],"only to bitmap"),(vec!["vectorize","input image.bmp","--format","png","--flatten"],"only to non-PNG"),(vec!["vectorize","input image.bmp","--format","png","--scale","0"],"positive finite")]{failure(&invoke(root,&args,None),error);}
    for plan in ["{","{}","[null]","[{\"action\":\"unknown\"}]","[{\"settings\":{\"smothness\":9}}]"]{std::fs::write(root.join("plan.json"),plan).unwrap();std::fs::write(root.join("protected.svg"),b"keep").unwrap();let failed=invoke(root,&["vectorize","input image.bmp","--edits","plan.json","-o","protected.svg","--force"],None);assert_eq!(failed.status.code(),Some(1));assert!(failed.stdout.is_empty());assert_eq!(std::fs::read(root.join("protected.svg")).unwrap(),b"keep");}
}

// App editing uses straight RGB before refitting, unlike the historical raw
// numerical capture. Compare every resulting SVG byte with the separate JS
// implementation, including recipes whose original native output was invalid.
fn application_edit_svg(reference:&Value)->Vec<u8>{
    let helper=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/helpers/application-edit-reference.mjs");
    let actions=reference["actions"].as_array().unwrap().iter().filter(|action|action["action"]!=5).cloned().collect::<Vec<_>>();
    let recipe=serde_json::to_vec(&json!({"input":reference["input"],"actions":actions})).unwrap();
    let mut child=Command::new("node").arg(helper).arg("--direct-stdin")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .expect("Node.js 22.19+ is required for the independent application-edit reference.");
    let mut stdin=child.stdin.take().unwrap();
    let (output,written)=std::thread::scope(|scope|{
        let writer=scope.spawn(move||stdin.write_all(&recipe));
        let output=child.wait_with_output().unwrap();
        (output,writer.join().unwrap())
    });
    written.expect("Send recipe inputs to independent application-edit reference.");
    success(&output);
    output.stdout
}
#[test]
fn native_segmentation_edit_recipes(){
    let directory=Directory::new();let root=&directory.0;
    let rows=common::read_ndjson("segmentation-edit-reference.ndjson.gz");
    for reference in rows.iter().skip(1){
        let input_name=reference["input"]["name"].as_str().unwrap();std::fs::write(root.join(input_name),common::b64(reference["input"]["bytes"].as_str().unwrap())).unwrap();
        let commands=reference["actions"].as_array().unwrap().iter().filter(|a|a["action"]!=5).map(|a|{
            if a["action"]!=1{return json!({"action":match a["action"].as_i64().unwrap(){2=>"undo",3=>"redo",4=>"revert",_=>panic!("unknown native action")}})}
            let color=a["argb"].as_array().unwrap().iter().rev().cloned().collect::<Vec<_>>();
            if a["kind"]==3{let c=a["coordinates"].as_array().unwrap();json!({"tool":"pencil","from":&c[..2],"to":&c[2..],"color":color,"commit":a["commit"]})}else{json!({"tool":if a["kind"]==5{"fill"}else{"zap"},"x":a["x"],"y":a["y"],"color":color,"commit":a["commit"]})}
        }).collect::<Vec<_>>();std::fs::write(root.join("edits.json"),serde_json::to_vec(&commands).unwrap()).unwrap();
        let output=invoke(root,&["vectorize",input_name,"--edits","edits.json","--layout","stacked"],None);success(&output);
        assert_eq!(output.stdout,application_edit_svg(reference),"{}",reference["id"]);
    }
}
#[test]
fn edit_plans_restore_committed_pixels_and_leave_the_pencil_tool(){
    let directory=Directory::new();let root=&directory.0;let (bytes,_)=automatic();
    std::fs::write(root.join("input.bmp"),&bytes).unwrap();
    let baseline=invoke(root,&["vectorize","input.bmp"],None);success(&baseline);
    let committed=json!([
        {"tool":"fill","x":1,"y":1,"color":[0,0,1,1]},
        {"action":"finish"}
    ]);
    std::fs::write(root.join("committed.json"),serde_json::to_vec(&committed).unwrap()).unwrap();
    let expected=invoke(root,&["vectorize","input.bmp","--edits","committed.json"],None);success(&expected);
    assert_ne!(expected.stdout,baseline.stdout,"the committed edit must change the exported document");
    let mut restored=committed.as_array().unwrap().clone();
    restored.extend([
        json!({"tool":"fill","x":1,"y":1,"color":[1,0,0,1]}),
        json!({"action":"restore-committed"}),
        json!({"gesture":{"phase":"select","tool":"pencil"}}),
        json!({"gesture":{"phase":"color","color":[1,0,0,1]}}),
        json!({"action":"leave"}),
        json!({"gesture":{"phase":"press","point":[4,4],"button":1,"buttons":1}}),
        json!({"gesture":{"phase":"move","point":[12,12],"button":0,"buttons":1}}),
        json!({"gesture":{"phase":"release","point":[12,12],"button":1,"buttons":0}})
    ]);
    std::fs::write(root.join("restored.json"),serde_json::to_vec(&restored).unwrap()).unwrap();
    let output=invoke(root,&["vectorize","input.bmp","--edits","restored.json"],None);success(&output);
    assert_eq!(output.stdout,expected.stdout,"discard the later fill and do not paint after leaving Pencil");
    assert_eq!(std::fs::read(root.join("input.bmp")).unwrap(),bytes);
}
#[test]
fn profiles_real_persistence_and_conversion(){
    let directory=Directory::new();let root=&directory.0;let (bytes,_)=automatic();std::fs::write(root.join("input.bmp"),bytes).unwrap();
    let output=invoke(root,&["profiles","list","--file","profiles.json"],None);success(&output);assert!(!root.join("profiles.json").exists());let entries:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(entries[0]["name"],"Default Advanced Settings");
    let profile=openvector::settings_profile::default_advanced_profile().to_json();let encoded=serde_json::to_vec(&profile).unwrap();
    let output=invoke(root,&["profiles","save","--file","profiles.json","--name","Logo α","--input","-"],Some(&encoded));success(&output);assert!(root.join("profiles.json").exists());
    reject_stdin(root,&["profiles","save","--file","profiles.json","--name","Logo α","--input","-"],&encoded,"--force");
    let output=invoke(root,&["profiles","select","--file","profiles.json","--name","Logo α"],None);success(&output);let selected:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(selected["index"],1);
    success(&invoke(root,&["profiles","rename","--file","profiles.json","--name","Logo α","--new-name"," Renamed "],None));
    let output=invoke(root,&["vectorize","input.bmp","--profile"," Renamed ","--profiles-file","profiles.json"],None);success(&output);assert!(output.stdout.starts_with(b"<?xml"));
    success(&invoke(root,&["profiles","delete","--file","profiles.json","--name"," Renamed "],None));failure(&invoke(root,&["profiles","show","--file","profiles.json","--name"," Renamed "],None),"Profile not found:");
}
#[test]
fn batch_real_failures_duplicates_overwrite_and_cancel(){
    let directory=Directory::new();let root=&directory.0;let (bytes,_)=automatic();std::fs::write(root.join("good.bmp"),bytes).unwrap();std::fs::write(root.join("bad.bmp"),b"invalid image").unwrap();std::fs::create_dir(root.join("out")).unwrap();
    let result=invoke(root,&["batch","good.bmp","bad.bmp","good.bmp","--output-dir","out","--format","svg"],None);assert_eq!(result.status.code(),Some(1));let summary:Value=serde_json::from_slice(&result.stdout).unwrap();assert_eq!(summary["status"],"complete");assert_eq!(summary["converted"],1);assert_eq!(summary["failed"],1);let stderr=String::from_utf8_lossy(&result.stderr);assert!(stderr.contains("Duplicate input skipped: good.bmp"));assert!(stderr.contains("Failed To Open File."));
    let before=std::fs::read(root.join("out/good.svg")).unwrap();let result=invoke(root,&["batch","good.bmp","--output-dir","out","--format","svg"],None);success(&result);let summary:Value=serde_json::from_slice(&result.stdout).unwrap();assert_eq!(summary["status"],"no-files");assert_eq!(summary["skipped"],1);
    let result=invoke(root,&["batch","good.bmp","--output-dir","out","--format","svg","--overwrite","cancel"],None);assert_eq!(result.status.code(),Some(130));let summary:Value=serde_json::from_slice(&result.stdout).unwrap();assert_eq!(summary["status"],"cancelled");assert_eq!(std::fs::read(root.join("out/good.svg")).unwrap(),before);
    let result=invoke(root,&["batch","good.bmp","--output-dir","missing","--format","svg"],None);assert_eq!(result.status.code(),Some(1));assert!(!root.join("missing").exists());let summary:Value=serde_json::from_slice(&result.stdout).unwrap();assert_eq!(summary["files"][0]["status"],"failed-save");
}
#[test]
fn help_and_numeric_stdin_contract(){let directory=Directory::new();let root=&directory.0;for args in [vec!["--help"],vec!["vectorize","--bogus","--help"],vec!["help","profiles"],vec!["batch","--help"],vec!["--version"]]{success(&invoke(root,&args,None));}
    let output=invoke(root,&["fill","-","0","2"],Some(br#"{"width":2,"height":2,"segments":[0,1,0,1]}"#));success(&output);assert_eq!(serde_json::from_slice::<Value>(&output.stdout).unwrap(),json!({"width":2,"height":2,"segments":[2,1,2,1]}));
    failure(&invoke(root,&["fill","-","+1","2"],Some(br#"{"width":2,"height":2,"segments":[0,1,0,1]}"#)),"seed must be an integer");}
