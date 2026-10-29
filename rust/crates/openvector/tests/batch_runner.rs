mod common;
mod editor_support;
use openvector::{batch_queue::{BatchQueue,OverwriteDecision},batch_runner::{BatchRunner,BatchEvent},image_document::CANCEL_TASK,palette::CoreSession};
use serde_json::json;
use std::{path::{Path,PathBuf},sync::atomic::{AtomicU64,Ordering},time::Instant};
struct Directory(PathBuf);
impl Directory {fn new()->Self{static NEXT:AtomicU64=AtomicU64::new(0);let p=std::env::temp_dir().join(format!("openvector-batch-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir(&p).unwrap();Self(p)}}
impl Drop for Directory{fn drop(&mut self){std::fs::remove_dir_all(&self.0).unwrap();}}
fn fixture_path(root:&Path,relative:&str)->PathBuf{
    // Fixture names use '/', but captured inputs use native separators. Queue identity preserves the supplied string.
    root.join(relative.replace('/',std::path::MAIN_SEPARATOR_STR))
}
fn normalize_log(log:&str,elapsed:Option<u128>)->String{
    log.split('\n').map(|line|{
        if let Some((label,value))=line.split_once(": "){
            if ["Start time","End time"].contains(&label){assert_eq!(value.len(),8);assert_eq!(&value[2..3],":");assert_eq!(&value[5..6],":");return format!("{label}: <clock>")}
            if ["Done classifying","Done finding palette","Done segmenting","Done smoothing","Done fitting","Elapsed time"].contains(&label){
                let (minutes,rest)=value.split_once(':').unwrap();let (seconds,millis)=rest.split_once('.').unwrap();assert_eq!(seconds.len(),2);assert_eq!(millis.len(),3);let seconds=seconds.parse::<u128>().unwrap();assert!(seconds<60);let duration=minutes.parse::<u128>().unwrap()*60000+seconds*1000+millis.parse::<u128>().unwrap();if let Some(elapsed)=elapsed{assert!(duration<=elapsed+1000)}return format!("{label}: <duration>")
            }
        }line.to_owned()
    }).collect::<Vec<_>>().join("\n")
}
fn normalize_eps(bytes:&[u8])->String{let text=std::str::from_utf8(bytes).unwrap();let mut clocks=0;let result=text.split_inclusive('\n').map(|line|if line.starts_with("%%CreationDate: "){clocks+=1;"%%CreationDate: <clock>\r\n"}else{line}).collect();assert_eq!(clocks,1);result}
#[test]
fn native_batch_runner_files_logs_and_task_boundaries(){
    let rows=common::read_ndjson("batch-runner-reference.ndjson.gz");let prefix=rows[0]["evidencePrefix"].as_str().unwrap();assert!(rows.len()>1);
    for reference in &rows[1..]{
        let id=reference["id"].as_str().unwrap();let directory=Directory::new();let root=&directory.0;let root_text=root.to_str().unwrap();
        let relocate=|path:&str|path.replace(prefix,root_text).replace(&prefix.replace('\\',"/"),root_text).replace('\\',std::path::MAIN_SEPARATOR_STR);
        let mut queue=BatchQueue::new(root_text,root_text);
        for source in reference["sources"].as_array().unwrap(){let path=fixture_path(root,source["name"].as_str().unwrap());std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,common::b64(source["bytes"].as_str().unwrap())).unwrap();queue.add_files(&[path.to_str().unwrap().to_owned()]);}
        let output=fixture_path(root,reference["exportDirectory"].as_str().unwrap());if reference["directoryExists"]==true{std::fs::create_dir_all(&output).unwrap();}
        queue.export_directory=output.to_str().unwrap().into();queue.export_format=reference["format"].as_str().unwrap().into();
        for (name,bytes) in reference["initialOutputs"].as_object().unwrap(){std::fs::write(output.join(name),common::b64(bytes.as_str().unwrap())).unwrap();}
        let mut runner=BatchRunner::new(queue,CoreSession::default());
        for settings in reference["priorSettings"].as_array().unwrap(){let prior=runner.run(settings,&json!({}),|_|OverwriteDecision::Skip,None).unwrap();assert_eq!(prior.status,"complete","{id}");assert_eq!(prior.failed,0,"{id}");}
        let mut decisions=0;let mut appeared=false;let mut stages=Vec::new();let mut last_export=None;
        let began=Instant::now();
        let result=runner.run(&reference["settings"],&json!({}),|paths|{decisions+=1;assert!(!paths.is_empty());OverwriteDecision::try_from(reference["decision"].as_str().unwrap()).unwrap()},Some(&mut|event:&BatchEvent|{
            if event.progress.is_some(){return Ok(())}
            let task=match event.stage{"load"=>8,"classification"=>1,"palette"=>2,"segmentation"=>3,"smoothing"=>4,"fitting"=>5,"export"=>6,_=>panic!("unknown batch stage")};
            stages.push(json!([event.index,task,event.state.status,event.state.progress,u8::from(event.state.cancelled),event.state.random_state]));
            let appearance=&reference["filesystemAppearance"];
            if !appearance.is_null()&&!appeared&&appearance["index"].as_u64()==Some(event.index as u64){let native=&reference["expected"]["processing"]["outputs"][event.index][0];let target=relocate(native.as_str().unwrap());use std::io::Write;std::fs::OpenOptions::new().create_new(true).write(true).open(target).unwrap().write_all(&common::b64(appearance["bytes"].as_str().unwrap())).unwrap();appeared=true;}
            if event.stage=="export"{last_export=Some(event.state.clone());}Ok(())
        })).unwrap_or_else(|e|panic!("{id}: {e}"));
        let elapsed=began.elapsed().as_millis();let native=&reference["expected"];let page=native["processing"]["wizardPage"].as_i64().unwrap();assert!((0..=3).contains(&page));
        let status=if page==1{"empty"}else if page==2||reference["decision"]=="cancel"{"cancelled"}else if page==0{"no-files"}else{"complete"};
        assert_eq!(result.status,status,"{id}");assert_eq!(result.queue.processing_index as u32 as u64,native["processingIndex"].as_u64().unwrap(),"{id}");
        let expected_flags=native["queue"].as_array().unwrap().iter().map(|r|r["initializedFlags"][0].as_i64().unwrap()!=0).collect::<Vec<_>>();assert_eq!(result.queue.queue.iter().map(|r|r.should_process).collect::<Vec<_>>(),expected_flags,"{id}");
        let expected_log=relocate(native["processing"]["log"].as_str().unwrap());assert_eq!(normalize_log(&result.log,Some(elapsed)),normalize_log(&expected_log,None),"{id}");
        assert_eq!(result.failed,expected_log.lines().filter(|line|["Failed To Open File.","Failed To Save File."].contains(line)).count(),"{id}");assert_eq!(result.converted,reference["expectedConverted"].as_u64().unwrap() as usize,"{id}");assert_eq!(result.skipped,expected_flags.iter().filter(|flag|!**flag).count(),"{id}");
        assert_eq!(result.files.iter().map(|f|f.output_filename.clone()).collect::<Vec<_>>(),native["processing"]["outputs"].as_array().unwrap().iter().map(|o|relocate(o[0].as_str().unwrap())).collect::<Vec<_>>(),"{id}");
        assert_eq!(result.files.iter().map(|f|f.filename.clone()).collect::<Vec<_>>(),native["queue"].as_array().unwrap().iter().map(|o|relocate(o["filename"].as_str().unwrap())).collect::<Vec<_>>(),"{id}");
        assert_eq!(decisions,usize::from(!reference["initialOutputs"].as_object().unwrap().is_empty()||!reference["priorSettings"].as_array().unwrap().is_empty()),"{id}");assert_eq!(appeared,!reference["filesystemAppearance"].is_null(),"{id}");
        if let Some(events)=reference["taskTrace"]["events"].as_array(){let expected=events.iter().filter(|e|e["kind"]==0).map(|e|json!([e["index"],e["task"],e["result"],e["progress"],e["cancelled"],e["randomState"]])).collect::<Vec<_>>();assert_eq!(stages,expected,"{id}");}
        if reference["directoryExists"]==true{
            let processed=result.files.iter().filter(|f|f.status=="written").map(|f|Path::new(&f.output_filename).file_name().unwrap().to_str().unwrap().to_owned()).collect::<Vec<_>>();
            let expected_outputs=reference["expectedOutputs"].as_object().unwrap();
            let mut expected_listing=expected_outputs.keys().cloned().collect::<std::collections::BTreeSet<_>>();
            // Case aliases share a physical file on Windows, but can remain
            // separate outputs on a case-sensitive host filesystem.
            for name in &processed {
                let path=output.join(name).canonicalize().unwrap();
                if !expected_outputs.keys().any(|native|output.join(native).canonicalize().unwrap()==path){expected_listing.insert(name.clone());}
            }
            let actual_listing=std::fs::read_dir(&output).unwrap().map(|e|e.unwrap().file_name().into_string().unwrap()).collect::<std::collections::BTreeSet<_>>();assert_eq!(actual_listing,expected_listing,"{id}");
            for (name,bytes) in expected_outputs{let holder=processed.iter().rev().find(|p|p.eq_ignore_ascii_case(name)).map(String::as_str).unwrap_or(name);let actual=std::fs::read(output.join(holder)).unwrap();let expected=common::b64(bytes.as_str().unwrap());if reference["format"]=="eps"{assert_eq!(normalize_eps(&actual),normalize_eps(&expected),"{id} {name}")}else{assert_eq!(actual,expected,"{id} {name}");}}
        }else{assert!(!output.exists(),"{id}");}
        if !native["document"].is_null(){editor_support::compare_document_state(last_export.as_ref().expect("missing final export state"),&native["document"]["state"],&rows[0]["parameters"]);}
    }
}

#[test]
fn fresh_native_batches_preserve_the_session_random_stream() {
    let rows = common::read_ndjson("fresh-batch-sequence-20261020.ndjson.gz");
    let reference = &rows[0];
    let directory = Directory::new();
    let root = directory.0.to_str().unwrap();
    let mut queue = BatchQueue::new(root, root);
    for input in reference["inputs"].as_array().unwrap() {
        let path = directory.0.join(input["name"].as_str().unwrap());
        std::fs::write(&path, common::b64(input["bytes"].as_str().unwrap())).unwrap();
        queue.add_files(&[path.to_str().unwrap().into()]);
    }
    queue.export_format = "svg".into();
    let mut runner = BatchRunner::new(queue, CoreSession::default());
    let settings = openvector::settings_profile::batch_profile_controls(
        &openvector::settings_profile::default_batch_profile());
    // The same three inputs produce different curves on the next batch.
    // Compare equivalent native session histories, never independent cold runs.
    for (phase, expected) in reference["runs"][0]["outputs"].as_array().unwrap().iter().enumerate() {
        let output = directory.0.join(reference["phaseNames"][phase].as_str().unwrap());
        std::fs::create_dir(&output).unwrap();
        runner.queue.export_directory = output.to_str().unwrap().into();
        let result = runner.run(&settings, &json!({}), |_| panic!("fresh output directory"), None).unwrap();
        assert_eq!(result.status, "complete");
        assert_eq!((result.converted, result.failed, result.skipped), (3, 0, 0));
        for (name, capture) in expected.as_object().unwrap() {
            assert_eq!(std::fs::read(output.join(name)).unwrap(),
                common::b64(capture["bytes"].as_str().unwrap()), "batch {phase}: {name}");
        }
    }
}

#[test]
fn native_batch_cancellation_and_same_session_retries(){
    let rows=common::read_ndjson("batch-cancellation-reference.ndjson.gz");
    let prefix=rows[0]["evidencePrefix"].as_str().unwrap();
    for reference in &rows[1..]{
        let id=reference["id"].as_str().unwrap();let directory=Directory::new();let root=&directory.0;let root_text=root.to_str().unwrap();
        let relocate=|path:&str|path.replace(prefix,root_text).replace(&prefix.replace('\\',"/"),root_text).replace('\\',std::path::MAIN_SEPARATOR_STR);
        let mut queue=BatchQueue::new(root_text,root_text);
        for source in reference["sources"].as_array().unwrap(){let path=fixture_path(root,source["name"].as_str().unwrap());std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,common::b64(source["bytes"].as_str().unwrap())).unwrap();queue.add_files(&[path.to_str().unwrap().into()]);}
        let output=fixture_path(root,reference["exportDirectory"].as_str().unwrap());if reference["directoryExists"]==true{std::fs::create_dir_all(&output).unwrap();}
        queue.export_directory=output.to_str().unwrap().into();queue.export_format=reference["format"].as_str().unwrap().into();
        for (name,bytes) in reference["initialOutputs"].as_object().unwrap(){std::fs::write(output.join(name),common::b64(bytes.as_str().unwrap())).unwrap();}
        let mut runner=BatchRunner::new(queue,CoreSession::default());
        for job in reference["runs"].as_array().unwrap(){
            let cancel=runner.cancellation_handle();cancel.store(false,Ordering::Relaxed);
            let mut stages=Vec::new();let mut decisions=0;let began=Instant::now();
            let result=runner.run(&job["settings"],&json!({}),|paths|{decisions+=1;assert_eq!(paths.len(),job["initialOutputs"].as_object().unwrap().len());OverwriteDecision::try_from(job["decision"].as_str().unwrap()).unwrap()},Some(&mut|event:&BatchEvent|{
                if event.progress.is_some(){return Ok(())}
                let task=match event.stage{"load"=>8,"classification"=>1,"palette"=>2,"segmentation"=>3,"smoothing"=>4,"fitting"=>5,"export"=>6,_=>panic!("unknown stage")};
                stages.push(json!([event.index,task,event.state.status,event.state.progress,u8::from(event.state.cancelled),event.state.random_state]));
                if job["cancelAfter"]["index"].as_u64()==Some(event.index as u64)&&job["cancelAfter"]["task"]==task{cancel.store(true,Ordering::Relaxed);}Ok(())
            })).unwrap_or_else(|e|panic!("{id}: {e}"));
            let native=&job["expected"];let expected_log=relocate(native["processing"]["log"].as_str().unwrap());
            assert_eq!(normalize_log(&result.log,Some(began.elapsed().as_millis())),normalize_log(&expected_log,None),"{id}");
            let page=native["processing"]["wizardPage"].as_i64().unwrap();let status=if page==1{"empty"}else if page==2||job["decision"]=="cancel"{"cancelled"}else if page==0{"no-files"}else{"complete"};
            assert_eq!(result.status,status,"{id}");assert_eq!(result.converted,job["expectedConverted"].as_u64().unwrap() as usize,"{id}");
            assert_eq!(result.failed,expected_log.lines().filter(|line|["Failed To Open File.","Failed To Save File."].contains(line)).count(),"{id}");
            assert_eq!(result.queue.processing_index as u32 as u64,native["processingIndex"].as_u64().unwrap(),"{id}");
            let flags=native["queue"].as_array().unwrap().iter().map(|r|r["initializedFlags"][0].as_i64().unwrap()!=0).collect::<Vec<_>>();
            assert_eq!(result.queue.queue.iter().map(|r|r.should_process).collect::<Vec<_>>(),flags,"{id}");assert_eq!(result.skipped,flags.iter().filter(|f|!**f).count(),"{id}");
            let expected_stages=job["taskTrace"]["events"].as_array().unwrap().iter().filter(|e|e["kind"]==0).map(|e|json!([e["index"],e["task"],e["result"],e["progress"],e["cancelled"],e["randomState"]])).collect::<Vec<_>>();
            assert_eq!(stages,expected_stages,"{id}");assert_eq!(decisions,usize::from(!job["initialOutputs"].as_object().unwrap().is_empty()),"{id}");
            let (running,queue,state)=runner.snapshot();assert!(!running);assert_eq!(queue,result.queue);
            editor_support::compare_document_state(state.as_ref().unwrap(),&native["document"]["state"],&rows[0]["parameters"]);
            let mut detached=state.unwrap();detached.source.bgra.fill(0);detached.settings.palette.fill(0.0);
            editor_support::compare_document_state(&runner.document().unwrap().snapshot(),&native["document"]["state"],&rows[0]["parameters"]);
            let expected_outputs=job["expectedOutputs"].as_object().unwrap();
            if reference["directoryExists"]==true{
                let actual_names=std::fs::read_dir(&output).unwrap().map(|e|e.unwrap().file_name().into_string().unwrap()).collect::<std::collections::BTreeSet<_>>();
                assert_eq!(actual_names,expected_outputs.keys().cloned().collect(),"{id}");
                for (name,bytes) in expected_outputs{assert_eq!(std::fs::read(output.join(name)).unwrap(),common::b64(bytes.as_str().unwrap()),"{id} {name}");}
            }else{
                assert!(expected_outputs.is_empty(),"{id}: absent export directory cannot have outputs");
                assert!(!output.exists(),"{id}: missing export directory must remain absent");
            }
        }
    }
}

#[test]
fn checkpoint_cancellation_and_observer_errors_keep_retry_available(){
    let reference=common::read_ndjson("batch-runner-reference.ndjson.gz").into_iter().find(|r|r["id"]=="uniform-pair").unwrap();
    for (stage,phase,substep) in [("palette",0,0),("segmentation",2,-1),("segmentation",6,-2),("smoothing",8,1),("smoothing",8,-2),("fitting",10,2),("fitting",10,-2)]{
        let directory=Directory::new();let root=&directory.0;let root_text=root.to_str().unwrap();let mut queue=BatchQueue::new(root_text,root_text);
        for source in reference["sources"].as_array().unwrap(){let path=fixture_path(root,source["name"].as_str().unwrap());std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,common::b64(source["bytes"].as_str().unwrap())).unwrap();queue.add_files(&[path.to_str().unwrap().into()]);}
        let output=root.join("out");std::fs::create_dir(&output).unwrap();queue.export_directory=output.to_str().unwrap().into();queue.export_format="svg".into();
        let mut runner=BatchRunner::new(queue,CoreSession::default());let cancel=runner.cancellation_handle();
        let before=runner.queue.snapshot();cancel.store(true,Ordering::Relaxed);let aborted=runner.run(&json!({}),&json!({}),|_|panic!("pre-aborted preflight"),None).unwrap();assert_eq!(aborted.status,"cancelled");assert!(aborted.files.is_empty());assert!(aborted.log.is_empty());assert_eq!(before,runner.queue.snapshot());assert!(runner.document().is_none());cancel.store(false,Ordering::Relaxed);
        let settings=json!({"category":2,"quality":0,"paletteMode":if stage=="palette"{1}else{2},"colorCount":2,"backgroundMode":1});
        let mut reached=false;
        let result=runner.run(&settings,&json!({}),|_|OverwriteDecision::Skip,Some(&mut|event:&BatchEvent|{
            if let Some(progress)=event.progress{if progress.stage==stage&&progress.phase==phase&&progress.substep==substep{reached=true;cancel.store(true,Ordering::Relaxed);}}Ok(())
        })).unwrap();
        assert!(reached,"{stage} {phase} {substep}");assert_eq!(result.status,"cancelled");assert!(result.files.iter().all(|f|f.status=="pending"));assert_eq!(std::fs::read_dir(&output).unwrap().count(),0);assert!(!result.log.contains("Done processing images."));assert!(!result.log.contains("End time:"));
        let state=runner.document().unwrap().snapshot();assert!(state.cancelled);
        if stage=="palette"{assert_eq!(state.status,0);assert_eq!(state.progress,0);assert_eq!(state.completed[2],0);}
        else{assert_eq!(state.status,2);assert_eq!(state.progress,100);}
        cancel.store(false,Ordering::Relaxed);
        let error=runner.run(&settings,&json!({}),|_|OverwriteDecision::Skip,Some(&mut|_:&BatchEvent|Err("observer failure".into()))).unwrap_err();assert_eq!(error,"observer failure");assert!(!runner.snapshot().0);assert_eq!(std::fs::read_dir(&output).unwrap().count(),0);
        let retry=runner.run(&settings,&json!({}),|_|OverwriteDecision::Skip,None).unwrap();assert_eq!(retry.status,"complete");assert_eq!(retry.converted,2);assert!(!runner.document().unwrap().snapshot().cancelled);
    }
}

#[test]
fn reserved_observer_cancellation_preserves_partial_report_and_retry() {
    let reference = common::read_ndjson("batch-runner-reference.ndjson.gz")
        .into_iter().find(|r| r["id"] == "uniform-pair").unwrap();
    for observer_error in [CANCEL_TASK, "observer failure"] {
        let directory = Directory::new();
        let root = &directory.0;
        let root_text = root.to_str().unwrap();
        let mut queue = BatchQueue::new(root_text, root_text);
        for source in reference["sources"].as_array().unwrap() {
            let path = fixture_path(root, source["name"].as_str().unwrap());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, common::b64(source["bytes"].as_str().unwrap())).unwrap();
            queue.add_files(&[path.to_str().unwrap().into()]);
        }
        let output = root.join("out");
        std::fs::create_dir(&output).unwrap();
        queue.export_directory = output.to_str().unwrap().into();
        queue.export_format = "svg".into();
        let mut runner = BatchRunner::new(queue, CoreSession::default());
        let cancel = runner.cancellation_handle();
        let mut reached = false;
        // Cancel through the reserved observer error, without setting the runner flag.
        // The first image is already saved when the second classification completes.
        let outcome = runner.run(&reference["settings"], &json!({}),
            |_| panic!("fresh output directory"), Some(&mut |event: &BatchEvent| {
                if event.index == 1 && event.stage == "classification" && event.progress.is_none() {
                    reached = true;
                    return Err(observer_error.into());
                }
                Ok(())
            }));
        assert!(reached);
        assert!(!cancel.load(Ordering::Relaxed));
        let (running, queue, _) = runner.snapshot();
        assert!(!running);
        assert_eq!(queue.processing_index, 1);
        let first_output = Path::new(&queue.queue[0].output_filename);
        let pending_output = Path::new(&queue.queue[1].output_filename);
        let written = std::fs::read(first_output).unwrap();
        assert_eq!(written, common::b64(reference["expectedOutputs"]["a.svg"].as_str().unwrap()));
        assert!(!pending_output.exists());
        if observer_error == CANCEL_TASK {
            let result = outcome.unwrap();
            assert_eq!(result.status, "cancelled");
            assert_eq!((result.converted, result.failed, result.skipped), (1, 0, 0));
            assert_eq!(result.files.iter().map(|f| f.status.as_str()).collect::<Vec<_>>(),
                ["written", "pending"]);
            assert!(result.files.iter().all(|f| f.error.is_none()));
            assert_eq!(result.queue, queue);
            assert!(result.queue.queue.iter().all(|r| r.should_process));
            assert!(result.log.contains("a.png"));
            assert!(result.log.contains("b.png"));
            assert!(!result.log.contains("End time:"));
            assert!(!result.log.contains("Done processing images."));
        } else {
            assert_eq!(outcome.unwrap_err(), observer_error);
        }
        // Ordinary preflight skips the saved output and processes the pending image.
        let retry = runner.run(&reference["settings"], &json!({}), |paths| {
            assert_eq!(paths, &[queue.queue[0].output_filename.clone()]);
            OverwriteDecision::Skip
        }, None).unwrap();
        assert_eq!(retry.status, "complete");
        assert_eq!((retry.converted, retry.failed, retry.skipped), (1, 0, 1));
        assert_eq!(retry.files.iter().map(|f| f.status.as_str()).collect::<Vec<_>>(),
            ["skipped", "written"]);
        assert_eq!(std::fs::read(first_output).unwrap(), written);
        assert_eq!(std::fs::read(pending_output).unwrap(),
            common::b64(reference["expectedOutputs"]["b.svg"].as_str().unwrap()));
    }
}
