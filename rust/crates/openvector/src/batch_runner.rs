//! Filesystem batch execution over the shared retained document and random stream.
use crate::{batch_queue::{BatchQueue,BatchSnapshot,OverwriteDecision},document_controls::{read_batch_controls,BatchControls,ADVANCED_BATCH_CONTROLS},export_options::{read_export_options,ExportOptions},image_document::{ImageDocument,DocumentSource,DocumentSnapshot,DocumentProgress,CANCEL_TASK},palette::CoreSession};
use serde::Serialize;
use serde_json::Value;
use std::{cell::RefCell,path::{Path,PathBuf},sync::{Arc,atomic::{AtomicBool,Ordering}},time::Instant};

#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct BatchFile { pub filename:String,pub output_filename:String,pub status:String,#[serde(skip_serializing_if="Option::is_none")]pub error:Option<String> }
#[derive(Clone,Debug,Serialize)]
pub struct BatchResult { pub status:String,pub files:Vec<BatchFile>,pub queue:BatchSnapshot,pub log:String,pub converted:usize,pub failed:usize,pub skipped:usize }
/// `palette_count` is the selected processing profile's count, not the live
/// palette length. Only stage-completion events carry a new selection.
pub struct BatchEvent<'a> { pub index:usize,pub filename:&'a str,pub stage:&'a str,pub state:&'a DocumentSnapshot,pub progress:Option<&'a DocumentProgress>,pub source:Option<(&'a str,bool)>,pub palette_count:Option<usize> }
pub type BatchObserver<'a> = dyn FnMut(&BatchEvent<'_>)->Result<(),String>+'a;
pub fn read_batch_configuration(settings:&Value,options:&Value)->Result<(BatchControls,ExportOptions),String>{
    let object=settings.as_object().ok_or("Batch settings must be an object.")?;
    for key in object.keys(){if !["advanced","category","quality","paletteMode","colorCount","backgroundMode","palette"].contains(&key.as_str())&&!ADVANCED_BATCH_CONTROLS.contains(&key.as_str()){return Err(format!("Unknown batch setting: {key}"))}}
    Ok((read_batch_controls(settings).map_err(|e|e.to_string())?,read_export_options(options,ExportOptions::default()).map_err(|e|e.to_string())?))
}
pub struct BatchRunner { pub queue:BatchQueue,document:ImageDocument,running:bool,cancel:Arc<AtomicBool>,has_document:bool }
impl BatchRunner {
    pub fn new(queue:BatchQueue,session:CoreSession)->Self{let mut runner=Self::from_document(queue,ImageDocument::new(session));runner.has_document=false;runner}
    pub fn from_document(queue:BatchQueue,document:ImageDocument)->Self{Self{queue,document,running:false,cancel:Arc::new(AtomicBool::new(false)),has_document:true}}
    pub fn document(&self)->Option<&ImageDocument>{self.has_document.then_some(&self.document)}
    pub fn into_document(self)->ImageDocument{self.document}
    pub fn cancellation_handle(&self)->Arc<AtomicBool>{self.cancel.clone()}
    pub fn snapshot(&self)->(bool,BatchSnapshot,Option<DocumentSnapshot>){(self.running,self.queue.snapshot(),self.document().map(ImageDocument::snapshot))}
    pub fn run(&mut self,settings:&Value,export_options:&Value,mut decide:impl FnMut(&[String])->OverwriteDecision,observer:Option<&mut BatchObserver>)->Result<BatchResult,String>{
        let (controls,options)=read_batch_configuration(settings,export_options)?;
        if !["svg","eps","pdf","ai","dxf"].contains(&self.queue.export_format.as_str()){return Err("Batch format must be svg, eps, pdf, ai or dxf.".into())}
        if self.running{return Err("This batch runner is already processing a queue.".into())}
        self.running=true;
        let mut files=Vec::new();let mut sections=Vec::new();
        let outcome=self.execute(settings,&controls,options,&mut decide,observer,&mut files,&mut sections);
        self.running=false;
        let status=match outcome{Ok(status)=>status,Err(error) if error==CANCEL_TASK=>"cancelled",Err(error)=>return Err(error)};
        Ok(BatchResult{status:status.into(),converted:files.iter().filter(|f|f.status=="written").count(),failed:files.iter().filter(|f|f.status=="failed-load"||f.status=="failed-save").count(),skipped:files.iter().filter(|f|f.status=="skipped").count(),files,queue:self.queue.snapshot(),log:sections.join("\n\n")})
    }
    fn execute(&mut self,settings:&Value,controls:&BatchControls,options:ExportOptions,decide:&mut dyn FnMut(&[String])->OverwriteDecision,observer:Option<&mut BatchObserver>,files:&mut Vec<BatchFile>,sections:&mut Vec<String>)->Result<&'static str,String>{
        if self.queue.snapshot().queue.is_empty(){return Ok("empty")}
        if self.cancel.load(Ordering::Relaxed){return Ok("cancelled")}
        let began=Instant::now();let root=PathBuf::from(&self.queue.working_directory);
        let plan=self.queue.prepare(|name|absolute(&root,name).exists());let before=self.queue.snapshot();
        sections.extend([format!("Starting batch processing of {} images...",before.queue.len()),format!("Start time: {}",clock()),format!("Export directory: '{}'",native_path(&before.export_directory))]);
        for record in &before.queue{sections.push(format!("Bitmap Filename: '{}'\nBitmap Directory: '{}'\nVector Filename: '{}'\nWould Overwrite: {}",basename(&record.filename),absolute(&root,&record.filename).parent().unwrap_or(Path::new("")).display(),basename(&record.output_filename),plan.overwrites.contains(&record.output_filename)));}
        let mut decision=OverwriteDecision::Skip;
        if !plan.overwrites.is_empty(){sections.push("Some files would be overwritten.\nAsking the user what to do...".into());decision=decide(&plan.overwrites);}
        if self.cancel.load(Ordering::Relaxed){decision=OverwriteDecision::Cancel;}
        let chosen=self.queue.decide_overwrite(decision);
        files.extend(self.queue.snapshot().queue.into_iter().map(|r|BatchFile{filename:r.filename,output_filename:r.output_filename,status:if r.should_process{"pending"}else{"skipped"}.into(),error:None}));
        if decision==OverwriteDecision::Cancel{return Ok("cancelled")}
        if !plan.overwrites.is_empty(){sections.last_mut().unwrap().push_str(if decision==OverwriteDecision::Overwrite{"\nThe user has decided to overwrite any existing files."}else{"\nThe user has decided to skip any files that would overwrite exising files."});}
        if !chosen.should_start{return Ok("no-files")}
        sections.extend(["Using the following settings:".into(),settings_log(controls,false),"Starting to process images...".into()]);
        let observer=RefCell::new(observer);let mut completing_export=false;
        for (index,file) in files.iter_mut().enumerate(){
            self.queue.processing_index=index as i32;if file.status=="skipped"{continue}
            let lines=RefCell::new(vec![format!("Converting '{}' to '{}'...",basename(&file.filename),basename(&file.output_filename))]);
            let outcome=(||->Result<(),String>{
                let metadata=crate::image_input::resolve_image_filename(&file.filename);
                let mut decoded_source=None;
                let cancel=self.cancel.clone();let document_cancel=self.document.cancellation_handle();let completing=completing_export;completing_export=false;self.has_document=true;
                let loaded=self.document.run_batch_load(settings,||{
                    if completing{document_cancel.store(true,Ordering::Relaxed);}
                    let encoded=std::fs::read(absolute(&root,&file.filename)).map_err(|e|e.to_string())?;
                    let decoded=crate::image_input::decode_image(&encoded).map_err(|e|e.to_string())?;
                    decoded_source=Some((decoded.format,decoded.has_alpha));
                    Ok(DocumentSource{width:decoded.width,height:decoded.height,bgra:decoded.bgra,has_alpha:decoded.has_alpha,shadow_control:metadata.shadow_control as f64})
                });
                let state=self.document.snapshot();
                if let Some(observe)=observer.borrow_mut().as_deref_mut(){observe(&BatchEvent{index,filename:&file.filename,stage:"load",state:&state,progress:None,source:if loaded.is_ok(){decoded_source}else{None},palette_count:None})?;}
                if cancel.load(Ordering::Relaxed){self.document.cancel_load();return Err(CANCEL_TASK.into())}
                if let Err(error)=loaded{file.status="failed-load".into();file.error=Some(error);lines.borrow_mut().push("Failed To Open File.".into());return Ok(())}
                let mut input=settings.clone();input["extension"]=Value::String(metadata.extension);
                let mut previous=Instant::now();
                let mut progress=|event:&DocumentProgress,state:&DocumentSnapshot|->Result<(),String>{if let Some(observe)=observer.borrow_mut().as_deref_mut(){observe(&BatchEvent{index,filename:&file.filename,stage:event.stage,state,progress:Some(event),source:None,palette_count:None})?;}if cancel.load(Ordering::Relaxed){document_cancel.store(true,Ordering::Relaxed);}Ok(())};
                let mut stage=|name:&str,result:&crate::image_document::TaskResult,selection:&crate::image_document::AutomaticSelection|->Result<(),String>{
                    if name=="segmentation"{let s=&result.state.settings;let detected=BatchControls{advanced:s.advanced,category:s.category,quality:s.quality,palette_mode:s.palette_mode,color_count:(s.palette.len()/4) as i32,background_mode:s.background_mode,palette:s.palette.clone(),overrides:Default::default()};let text=settings_log(&detected,true);if !text.is_empty(){lines.borrow_mut().push(text);}}
                    if let Some(observe)=observer.borrow_mut().as_deref_mut(){observe(&BatchEvent{index,filename:&file.filename,stage:name,state:&result.state,progress:None,source:None,palette_count:selection.palette_count})?;}
                    if cancel.load(Ordering::Relaxed){document_cancel.store(true,Ordering::Relaxed);return Err(CANCEL_TASK.into())}
                    let label=match name{"classification"=>"classifying","palette"=>"finding palette","segmentation"=>"segmenting","smoothing"=>"smoothing",_=>"fitting"};
                    lines.borrow_mut().push(format!("Done {label}: {}",duration(previous.elapsed())));previous=Instant::now();Ok(())
                };
                self.document.run_batch_image_with_observers(&input,Some(&mut progress),Some(&mut stage))?;
                let saved=self.document.save_vector_file(&absolute(&root,&file.output_filename),&options,Some(&mut |_|{
                    if cancel.load(Ordering::Relaxed){document_cancel.store(true,Ordering::Relaxed);}Ok(())
                }))?;
                let save_error=saved.error.map(|e|e.to_string());
                let exported=self.document.snapshot();
                completing_export=save_error.is_none();if save_error.is_none(){file.status="written".into();}
                if let Some(observe)=observer.borrow_mut().as_deref_mut(){observe(&BatchEvent{index,filename:&file.filename,stage:"export",state:&exported,progress:None,source:None,palette_count:None})?;}
                if cancel.load(Ordering::Relaxed){document_cancel.store(true,Ordering::Relaxed);return Err(CANCEL_TASK.into())}
                if let Some(error)=save_error{file.status="failed-save".into();file.error=Some(error);lines.borrow_mut().push("Failed To Save File.".into());}
                Ok(())
            })();
            sections.push(lines.into_inner().join("\n"));outcome?;
        }
        sections.extend([format!("End time: {}\nElapsed time: {}",clock(),duration(began.elapsed())),"Done processing images.".into()]);Ok("complete")
    }
}
fn absolute(root:&Path,name:&str)->PathBuf{let p=Path::new(name);if p.is_absolute(){p.into()}else{root.join(p)}}
fn basename(name:&str)->&str{Path::new(name).file_name().and_then(|n|n.to_str()).unwrap_or("")}
fn native_path(name:&str)->String{name.replace('/',std::path::MAIN_SEPARATOR_STR)}
fn duration(value:std::time::Duration)->String{let ms=value.as_millis();format!("{}:{:02}.{:03}",ms/60000,ms/1000%60,ms%1000)}
fn clock()->String{
    unsafe {
        let now=libc::time(std::ptr::null_mut());let mut output:libc::tm=std::mem::zeroed();
        #[cfg(unix)] let valid=!libc::localtime_r(&now,&mut output).is_null();
        #[cfg(windows)] let valid=libc::localtime_s(&mut output,&now)==0;
        if valid{format!("{:02}:{:02}:{:02}",output.tm_hour,output.tm_min,output.tm_sec)}else{String::new()}
    }
}
fn settings_log(s:&BatchControls,detected:bool)->String{
    let background=match s.background_mode{0=>"Preserve transparency",1=>"Flatten to white",3=>"Auto-detect",_=>"undefined"};
    if s.advanced{return if !detected{format!("Saved Advanced Settings\nTransparency Mode: {background}")}else if s.palette_mode==1{format!("Number of Colors: {}",s.palette.len()/4)}else{String::new()}}
    let categories=["Photograph","Artwork with blending","Artwork without blending","Auto-detect"];let qualities=["High","Medium","Low","Auto-detect"];let palettes=["Two colors","Fixed Palette","Flexible Palette","Auto-detect"];
    let mut lines=vec![format!("Image Category: {}",categories[s.category as usize]),format!("Image Quality: {}",qualities[s.quality as usize]),format!("Palette Mode: {}",palettes[s.palette_mode as usize])];
    if !detected||s.palette_mode!=2{let count=if detected{s.palette.len() as i32/4}else{s.color_count};lines.push(format!("Number of Colors: {}",if count==0{"Auto-detect".into()}else{count.to_string()}));}
    lines.push(format!("Transparency Mode: {background}"));lines.join("\n")
}
