use crate::{Arguments,Result,read_bytes,read_json,print_json,recent_index};
use openvector::{palette::CoreSession,image_document::{ImageDocument,TaskResult,AutomaticSelection,DocumentSnapshot},profile_palette::PaletteRuntime,saved_profiles::{SavedAdvancedProfiles,RecentBasicProfiles},settings_profile::{SettingsProfile,default_batch_profile,default_selected_profile,batch_profile_controls,apply_advanced_profile,apply_basic_profile,run_advanced_profile,run_basic_profile},batch_queue::{BatchQueue,OverwriteDecision},batch_runner::{BatchRunner,read_batch_configuration},bitmap_export::{BitmapExportInput,BitmapExportOptions,BitmapExportError,BitmapFormat}};
use serde_json::{json,Value};
use std::{path::{Path,PathBuf,Component},io::{Write,IsTerminal},fs::OpenOptions,sync::atomic::Ordering};
struct ProfileChoice{list:SavedAdvancedProfiles,filename:String,profile:SettingsProfile}
fn profile(args:&Arguments)->Result<Option<ProfileChoice>>{
    let (name,file)=(args.get("profile"),args.get("profiles-file"));
    if name.is_none()&&file.is_none(){return Ok(None)}
    let (name,file)=name.zip(file).ok_or("--profile and --profiles-file must be supplied together.")?;
    if file.is_empty()||file=="-"{return Err("--profiles-file must name a profile storage file.".into())}
    let mut list=SavedAdvancedProfiles::read_file(file)?;let (_,profile)=list.select(name);
    Ok(Some(ProfileChoice{list,filename:file.into(),profile:profile.ok_or_else(||format!("Profile not found: {name}"))?}))
}
/// Recent basic settings storage named by --recent-file.
struct RecentStore{list:RecentBasicProfiles,filename:String}
/// Reads recent storage during admission, before the image, and resolves the
/// replay entry. Only visible entries replay: native shows four links and
/// never exposes the fifth stored entry.
fn recent_store(args:&Arguments)->Result<(Option<RecentStore>,Option<SettingsProfile>)>{
    let index=args.get("recent-index").map(|value|recent_index(value,"--recent-index")).transpose()?;
    let Some(file)=args.get("recent-file") else{return if index.is_some(){Err("--recent-index requires --recent-file.".into())}else{Ok((None,None))}};
    if file.is_empty()||file=="-"{return Err("--recent-file must name a recent settings storage file.".into())}
    // Both stores are whole-file JSON documents; one shared path would overwrite the other.
    if let Some(profiles)=args.get("profiles-file"){if storage_paths_collide(profiles,file)?{return Err("Profile storage and recent settings storage must be different files.".into())}}
    let list=RecentBasicProfiles::read_file(file)?;
    let entry=index.map(|index|match list.visible_entries().get(index){
        Some(entry)=>Ok(entry.profile.clone()),
        None if index<list.entries().len()=>Err(format!("Recent settings entry {index} is stored but hidden; only the first {} can be replayed.",list.visible_entries().len())),
        None=>Err(format!("Recent settings entry not found: {index}")),
    }).transpose()?;
    Ok((Some(RecentStore{list,filename:file.into()}),entry))
}
/// Passes one fitting completion through the shared Rust recording rule and
/// persists an eligible one at once, like saved profile selection, so a later
/// failure in the same command keeps it. Vectorize never runs batch work.
fn record(recent:&mut Option<RecentStore>,profile:&SettingsProfile,status:u32,did_work:bool)->Result<()>{
    let Some(store)=recent else{return Ok(())};
    if store.list.record_completion(profile,status,did_work,false){store.list.write_file(&store.filename)?;}Ok(())
}
/// Automatic conversion tracking the selected profile like the GUI controller:
/// classification and palette selections replace its controls and background
/// recommendation, a chosen palette sets both color counters, and the fitting
/// completion records the result.
fn automatic(document:&mut ImageDocument,input:&Value,selected:&mut SettingsProfile,recent:&mut Option<RecentStore>)->Result<DocumentSnapshot>{
    let mut fitting=None;
    let mut on_stage=|stage:&str,task:&TaskResult,_:&AutomaticSelection|->Result<()>{if stage=="fitting"{fitting=Some((task.state.status,task.did_work));}Ok(())};
    let result=document.run_automatic_with_observers(input,None,Some(&mut on_stage))?;
    selected.settings=result.settings;selected.recommended_background_mode=result.recommended_background_mode;
    if let Some(count)=result.palette_count{selected.color_count=count as i32;selected.palette_color_count=count as i32;}
    let (status,did_work)=fitting.ok_or("Automatic conversion finished without a fitting completion.")?;
    record(recent,selected,status,did_work)?;
    Ok(result.state)
}
/// Edit finish commits edits and refits smoothing and fitting; its fitting
/// completion records the profile it processed.
fn finish(document:&mut ImageDocument,profile:&SettingsProfile,recent:&mut Option<RecentStore>)->Result<DocumentSnapshot>{
    let mut on_stage=|stage:&str,task:&TaskResult|if stage=="fitting"{record(recent,profile,task.state.status,task.did_work)}else{Ok(())};
    document.finish_editing_with_observers(&Value::Object(profile.settings.to_json()),None,Some(&mut on_stage))
}
fn absolute(path:&str)->Result<PathBuf>{
    let path=Path::new(path);let full=if path.is_absolute(){path.into()}else{std::env::current_dir().map_err(|e|e.to_string())?.join(path)};
    let mut normalized=PathBuf::new();for component in full.components(){match component{Component::CurDir=>{},Component::ParentDir=>{normalized.pop();},other=>normalized.push(other.as_os_str())}}Ok(normalized)
}
// Inputs are already files; a missing destination cannot alias one. Let the
// filesystem decide case sensitivity rather than folding input names.
fn source_output_conflict(source:&str,output:&str)->Result<bool>{
    match openvector::vector_file::same_file_identity(Path::new(source),Path::new(output)){
        Ok(same)=>Ok(same),
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>Ok(false),
        Err(error)=>Err(error.to_string()),
    }
}
// Storage paths may not exist yet, so keep lexical collision checks for that
// case, in addition to recognizing existing aliases.
fn storage_paths_collide(a:&str,b:&str)->Result<bool>{
    match openvector::vector_file::same_file_identity(Path::new(a),Path::new(b)){
        Ok(same)=>Ok(same),
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>{
            let (a,b)=(absolute(a)?,absolute(b)?);
            #[cfg(windows)] {Ok(a.to_string_lossy().to_lowercase()==b.to_string_lossy().to_lowercase())}
            #[cfg(not(windows))] {Ok(a==b)}
        },
        Err(error)=>Err(error.to_string()),
    }
}
fn source(document:&mut ImageDocument,filename:&str,encoded:&[u8],shrink:bool)->Result<String>{let metadata=openvector::image_input::resolve_image_filename(filename);let decoded=openvector::image_input::decode_image(encoded).map_err(|e|e.to_string())?;document.set_source(decoded.width,decoded.height,&decoded.bgra,decoded.has_alpha,metadata.shadow_control as f64)?;drop(decoded);if shrink{document.shrink_source();}Ok(metadata.extension)}
fn export_options(args:&Arguments)->Value{
    let mut options=json!({});
    for (flag,key) in [("layout","layout"),("dxf-mode","curveMode")]{if let Some(value)=args.get(flag){options[key]=json!(value);}}
    for (flag,key) in [("thin-stroke","thinStroke"),("compact","compact")]{if args.flag(flag){options[key]=json!(true);}}
    options
}
pub fn vectorize(args:&[String])->Result<()>{
    let args=Arguments::parse(args,&["output","format","filename","edits","profile","profiles-file","recent-file","recent-index","scale","layout","dxf-mode"],&["force","flatten","thin-stroke","compact","shrink-source"])?;
    if args.positionals.len()!=1{return Err("Choose exactly one input image. Use vectorize --help for syntax.".into())}
    let input=&args.positionals[0];
    if input=="-"&&args.get("filename").is_none_or(str::is_empty){return Err("Binary stdin requires --filename with the source image name.".into())}
    if input!="-"&&args.get("filename").is_some(){return Err("--filename is only valid with stdin (-).".into())}
    if args.get("recent-index").is_some()&&args.get("profile").is_some(){return Err("Choose --profile or --recent-index, not both.".into())}
    let output=args.get("output").filter(|v|*v!="-");
    let format=args.get("format").map(str::to_owned).unwrap_or_else(||output.map_or("svg",|p|Path::new(p).extension().and_then(|v|v.to_str()).unwrap_or("")).into()).to_lowercase();
    let bitmap=format.parse::<BitmapFormat>().ok();
    if bitmap.is_none()&&! ["svg","eps","pdf","ai","dxf","emf"].contains(&format.as_str()){return Err("Choose a format: SVG, EPS, PDF, AI, DXF, EMF, BMP, JPEG, PNG or TIFF. Use --format if the output extension is unknown.".into())}
    if args.get("scale").is_some()&&bitmap.is_none(){return Err("--scale applies only to bitmap output.".into())}
    if args.flag("flatten")&&(bitmap.is_none()||format=="png"){return Err("--flatten applies only to non-PNG bitmap output.".into())}
    let export_fields=export_options(&args);
    if bitmap.is_some()&&!export_fields.as_object().unwrap().is_empty(){return Err("--layout, --thin-stroke, --compact and --dxf-mode apply only to vector output.".into())}
    let export=openvector::export_options::read_export_options(&export_fields,openvector::export_options::ExportOptions::default()).map_err(|error|error.to_string())?;
    let scale=args.get("scale").map(|s|s.trim().parse::<f64>().unwrap_or(f64::NAN)).unwrap_or(1.0);
    if !scale.is_finite()||scale<=0.0{return Err("--scale must be a positive finite number.".into())}
    if let Some(output)=output{
        if input!="-"&&source_output_conflict(input,output)?{return Err("Input and output must be different files.".into())}
        if !args.flag("force")&&Path::new(output).exists(){return Err("Output already exists. Use --force to replace it.".into())}
        if let Some(file)=args.get("profiles-file"){if storage_paths_collide(file,output)?{return Err("Profile storage and output must be different files.".into())}}
        if let Some(file)=args.get("recent-file"){if storage_paths_collide(file,output)?{return Err("Recent settings storage and output must be different files.".into())}}
    }
    let (mut recent,recent_replay)=recent_store(&args)?;let choice=profile(&args)?;let encoded=read_bytes(input)?;let mut document=ImageDocument::new(CoreSession::default());
    let mut extension=source(&mut document,if input=="-"{args.get("filename").unwrap()}else{input},&encoded,args.flag("shrink-source"))?;
    if std::io::stderr().is_terminal(){eprintln!("Vectorizing image...");}
    // Each invocation is a fresh session. Replays replace the selected profile;
    // automatic and edit-plan selections update it as the GUI controller does.
    let mut selected=default_selected_profile();
    // Replays record the profile they processed; the automatic path tracks its own.
    let replay=if let Some(choice)=choice{
        document.commit_settings(&Value::Object(choice.profile.settings.to_json()))?;choice.list.write_file(&choice.filename)?;
        Some(run_advanced_profile(&mut document,&choice.profile,&PaletteRuntime::default(),None,None)?.ok_or("Saved profile uses auto-detect palette mode. Fast Forward does not process it; choose a fixed or flexible profile.")?)
    }else if let Some(entry)=recent_replay{
        // Selecting an entry records nothing; only the replay's fitting completion does.
        document.commit_settings(&Value::Object(apply_basic_profile(&entry).settings.to_json()))?;
        Some(run_basic_profile(&mut document,&entry,&PaletteRuntime::default(),None,None)?.ok_or("Recent settings entry does not choose two, fixed or unlimited colors, so replay does not process it.")?)
    }else{None};
    let processed=match replay{Some(replay)=>{record(&mut recent,&replay.profile,replay.state.status,replay.did_work["fitting"])?;selected=replay.profile;replay.state},None=>automatic(&mut document,&json!({"extension":extension}),&mut selected,&mut recent)?};
    let state=if let Some(plan)=args.get("edits"){
        drop(processed);
        let commands=read_json(plan)?;let commands=commands.as_array().ok_or("--edits must contain a JSON array of processing/edit commands.")?;
        document.begin_editing()?;
        for command in commands{
            let object=command.as_object().ok_or("Each edit command must be an object.")?;
            let kinds=["tool","gesture","action","background","settings","source"].into_iter().filter(|k|object.contains_key(*k)).collect::<Vec<_>>();
            if kinds.len()!=1{return Err("Select one command kind: tool, gesture, action, background, settings or source.".into())}
            match kinds[0]{
                "gesture"=>{document.gesture(&command["gesture"])?;},
                "background"=>background(&mut document,&command["background"] )?,
                "settings"=>{let settings=command["settings"].as_object().ok_or("Settings must be an object of processing controls.")?;let mut merged=selected.settings.to_json();for (key,value) in settings{if !merged.contains_key(key){return Err(format!("Unknown processing setting: {key}"))}merged.insert(key.clone(),value.clone());}selected.settings=openvector::document_controls::read_document_settings(&Value::Object(merged)).map_err(|e|e.to_string())?;},
                "source"=>{
                    let name=command["source"].as_str().filter(|v|!v.is_empty()).ok_or("Source must name an image file.")?;
                    let path=absolute(plan)?.parent().unwrap().join(name);let path=path.to_string_lossy();
                    if let Some(output)=output{if source_output_conflict(&path,output)?{return Err("Input and output must be different files.".into())}}
                    extension=source(&mut document,&path,&read_bytes(&path)?,args.flag("shrink-source"))?;
                    // A replaced source drops the palette, transparency and recommendation, like the GUI.
                    selected.settings.palette.clear();selected.settings.background_mode=-1;selected.settings.background=[1.0,1.0,1.0,0.0];selected.palette_color_count=2;selected.recommended_background_mode=-1;
                    let mut input=Value::Object(selected.settings.to_json());input["extension"]=json!(extension);automatic(&mut document,&input,&mut selected,&mut recent)?;document.begin_editing()?;
                },
                "action"=>match command["action"].as_str(){
                    Some("undo")=>{document.undo_edit()?;},Some("redo")=>{document.redo_edit()?;},Some("revert")=>{document.revert_edits()?;},Some("commit")=>{document.commit_editing()?;},Some("finish")=>{finish(&mut document,&selected,&mut recent)?;},Some("begin")=>{document.begin_editing()?;},
                    Some("restore-committed")=>{document.restore_committed_editing()?;},Some("leave")=>{document.leave_editing()?;},
                    Some("process")=>{let mut input=Value::Object(selected.settings.to_json());input["extension"]=json!(extension);input["lower"]=command["lower"].clone();input["upper"]=command["upper"].clone();let stage=command["stage"].as_str().unwrap_or("");let task=document.run_task(stage,&input,None)?;if stage=="fitting"{record(&mut recent,&selected,task.state.status,task.did_work)?;}},
                    _=>return Err("Edit action must be undo, redo, revert, begin, commit, restore-committed, leave, finish or process.".into())
                },
                _=>{document.edit(command)?;}
            }
        }
        // The closing refit processes the last committed settings, so it records those.
        if document.editing_state().is_some_and(|e|e.key!=document.state().stage_keys.segmentation_edits){selected.settings=document.state().settings.clone();finish(&mut document,&selected,&mut recent)?}else{document.snapshot()}
    }else{processed};
    let fitted=document.presentation_geometry().ok_or("Vector paths are unavailable. Update processing settings and run smoothing and fitting before exporting.")?.into_owned();
    let background=document.background_state();let hidden=background.hidden_regions.iter().map(|n|*n as i64).collect::<Vec<_>>();
    let bytes=if let Some(format)=bitmap{
        openvector::bitmap_export::export_bitmap(&BitmapExportInput{geometry:Some(&fitted),width:state.original.width as i64,height:state.original.height as i64,source_alpha_type:document.source_alpha_type(),background_mode:state.settings.background_mode,background_undo_count:background.undo.len(),hidden_regions:&hidden},&BitmapExportOptions{format,scale,flatten:args.flag("flatten")}).map_err(|e|match e{BitmapExportError::FlattenRequired=>"Non-PNG output may discard transparency. Use --flatten to continue, or --format png to preserve it.".into(),other=>other.to_string()})?
    }else{
        let mut request=json!({"width":state.original.width,"height":state.original.height,"palette":state.segmentation.as_ref().ok_or("No segmentation.")?.palette,"hiddenRegions":hidden,"layout":export.layout.name(),"thinStroke":export.thin_stroke,"compact":export.compact,"curveMode":export.curve_mode.name()});
        if format=="emf"{
            let device=openvector::emf::EmfDevice::current_display().map_err(|error|error.to_string())?;
            request["referenceDevice"]=json!({"pixels":device.pixels,"millimeters":device.millimeters,"micrometers":device.micrometers});
        }
        crate::numeric::export(&format,&fitted,&request)?
    };
    if let Some(output)=output{let mut options=OpenOptions::new();options.write(true);if args.flag("force"){options.create(true).truncate(true);}else{options.create_new(true);}options.open(output).and_then(|mut f|f.write_all(&bytes)).map_err(|e|e.to_string())?;}else{std::io::stdout().write_all(&bytes).map_err(|e|e.to_string())?;}Ok(())
}
fn background(document:&mut ImageDocument,input:&Value)->Result<()>{
    use openvector::background_editor::{BackgroundCommand,read_background_command};
    match read_background_command(input)?{BackgroundCommand::Begin=>{document.begin_background_editing()?;},BackgroundCommand::Toggle(point)=>{document.toggle_background(point)?;},BackgroundCommand::Undo=>{document.undo_background();},BackgroundCommand::Redo=>{document.redo_background();},BackgroundCommand::Revert=>{document.revert_background();},BackgroundCommand::Finish=>{document.finish_background_editing();},BackgroundCommand::Gesture(events)=>{for event in events{document.background_gesture(&event)?;}}}Ok(())
}
pub fn batch(args:&[String])->Result<i32>{
    let args=Arguments::parse(args,&["output-dir","format","settings","layout","dxf-mode","overwrite","profile","profiles-file"],&["thin-stroke","compact"])?;
    if args.positionals.is_empty()||args.positionals.iter().any(|f|f.is_empty()||f=="-"){return Err("Choose one or more image files. Use vectorize for binary stdin, or batch --help for syntax.".into())}
    let decision=match args.get("overwrite").unwrap_or("skip"){"skip"=>OverwriteDecision::Skip,"replace"=>OverwriteDecision::Overwrite,"cancel"=>OverwriteDecision::Cancel,_=>return Err("--overwrite must be skip, replace or cancel.".into())};
    if args.get("output-dir")==Some(""){return Err("--output-dir must name an existing export folder.".into())}
    let mut settings=args.get("settings").map(read_json).transpose()?.unwrap_or(json!({}));
    let cwd=std::env::current_dir().map_err(|e|e.to_string())?;let exe=std::env::current_exe().map_err(|e|e.to_string())?;
    let mut queue=BatchQueue::new(&exe.parent().unwrap().to_string_lossy(),&cwd.to_string_lossy());
    for duplicate in queue.add_files(&args.positionals){eprintln!("Duplicate input skipped: {duplicate}");}
    if let Some(directory)=args.get("output-dir"){queue.export_directory=directory.into();}if let Some(format)=args.get("format"){queue.export_format=format.to_lowercase();}
    let options=export_options(&args);
    if let Some(choice)=profile(&args)?{
        let (controls,_)=read_batch_configuration(&settings,&options)?;let mut base=default_batch_profile();let mut fields=base.settings.to_json();
        for (key,value) in &controls.overrides{fields.insert(key.clone(),value.clone());}
        for (key,value) in [("advanced",json!(controls.advanced)),("category",json!(controls.category)),("quality",json!(controls.quality)),("paletteMode",json!(controls.palette_mode)),("backgroundMode",json!(controls.background_mode)),("palette",json!(controls.palette))]{fields.insert(key.into(),value);}
        base.settings=openvector::document_controls::read_document_settings(&Value::Object(fields)).map_err(|e|e.to_string())?;
        if controls.advanced{base.color_count=controls.color_count}else{base.basic_color_count=controls.color_count}
        settings=batch_profile_controls(&apply_advanced_profile(Some(&base),&choice.profile,true)?);read_batch_configuration(&settings,&options)?;choice.list.write_file(&choice.filename)?;
    }
    if std::io::stderr().is_terminal(){eprintln!("Processing batch...");}
    let mut runner=BatchRunner::new(queue,CoreSession::default());let cancel=runner.cancellation_handle();
    ctrlc::set_handler(move||{if cancel.swap(true,Ordering::Relaxed){eprintln!("Forcing batch cancellation. An active output file may be incomplete.");std::process::exit(130);}else{eprintln!("Cancelling batch. Press Ctrl-C again to force termination.");}}).map_err(|e|e.to_string())?;
    let result=runner.run(&settings,&options,|_|decision,None)?;
    for file in &result.files{if file.status=="failed-load"||file.status=="failed-save"{eprintln!("{}: {} {}",file.filename,if file.status=="failed-load"{"Failed To Open File."}else{"Failed To Save File."},file.error.as_deref().unwrap_or(""));}}
    print_json(&serde_json::to_value(&result).map_err(|e|e.to_string())?)?;Ok(if result.status=="cancelled"{130}else if result.failed>0{1}else{0})
}
