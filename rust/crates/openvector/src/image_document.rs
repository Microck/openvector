//! Native document cache, live core and controller lifetimes.
use crate::{document_controls::*,palette::{CoreSession,PreparedPaletteAnalysis,AnalysisControls,AnalyzedPalette},image_resize::{OwnedImage,reduce_working_image},image_pipeline::{prepare_original_image,analyze_prepared_image,ImageState,AnalyzedImage},image_preparation::{composite_source_image,filter_prepared_image},settings::{resolve_parameters,NUMERIC_DATA},segmentation_state::{SegmentationState,SegmentationSnapshot},region_data::Raster,contours::{Geometry,ContourInput,extract_contours},contour_smoothing::{SmoothedContours,SmoothContourInput,smooth_contour_geometry},contour_fitting::{ContourFitter,FittedGeometry,FitterSnapshot,FittingParameters},vector_output::{OutputShapes,assemble_output_shapes},segmentation_editor::{SegmentationEditor,EditorSnapshot,EditorCheckpoint},background_editor::{BackgroundEditor,BackgroundSnapshot},editor_input::EditorPointer};
use serde_json::{Value,json};
use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
/// Reserved callback control signal, equivalent to requesting cancellation at
/// the current checkpoint. Other observer errors propagate without setting it.
pub const CANCEL_TASK:&str="Document task was cancelled.";
fn observer_result<T>(cancel:&AtomicBool,result:Result<T,String>)->Result<T,String>{
    if result.as_ref().is_err_and(|error|error==CANCEL_TASK){cancel.store(true,Ordering::Relaxed)}
    result
}
#[derive(Clone,Debug)]
pub struct DocumentSource{pub width:usize,pub height:usize,pub bgra:Vec<u8>,pub has_alpha:bool,pub shadow_control:f64}
#[derive(Clone,Debug,Default)]
pub struct DocumentClassification{pub category:i32,pub quality:i32,pub palette_mode:i32,pub color_count:i32}
#[derive(Clone,Debug,Default)]
pub struct StageKeys{pub palette:String,pub reserved_c0:String,pub segmentation:String,pub segmentation_edits:String,pub smoothing:String,pub fitting:String}
#[derive(Clone,Debug)]
pub struct DocumentFitting{pub complete:bool,pub geometry:FittedGeometry,pub cubic_availability:Vec<u8>,pub output_chains:Option<OutputShapes>}
/// Host-supplied output sizing, clock text and device metrics; omitted values
/// retain the serializer's native defaults.
#[derive(Clone,Debug,Default)]
pub struct DocumentExportEnvironment{pub output_size:Option<(i64,i64)>,pub creation_date:Option<String>,pub reference_device:Option<crate::emf::EmfDevice>}
#[derive(Clone,Debug)]
pub struct DocumentSnapshot{pub original:OwnedImage,pub source:OwnedImage,pub working:OwnedImage,pub settings:DocumentSettings,pub classification:DocumentClassification,pub parameters:Value,pub stage_keys:StageKeys,pub completed:[u8;25],pub scores:[f32;25],pub palettes:Vec<Vec<f32>>,pub segmentation:Option<SegmentationSnapshot>,pub contours:Option<Geometry>,pub smoothing:Option<SmoothedContours>,pub fitting:Option<DocumentFitting>,pub processed:bool,pub status:u32,pub progress:i32,pub cancelled:bool,pub random_state:u32}
#[derive(Clone,Debug)]
pub struct DocumentProgress{pub stage:&'static str,pub phase:i32,pub substep:i32,pub counter:i32,pub fraction:f64,pub percent:i32,pub color_count:Option<usize>,pub iteration:Option<usize>}
pub type DocumentObserver<'a>=dyn FnMut(&DocumentProgress,&DocumentSnapshot)->Result<(),String>+'a;
#[derive(Clone,Debug)]
pub struct TaskResult{pub did_work:bool,pub settings:DocumentSettings,pub state:DocumentSnapshot,pub analysis:Option<AnalyzedImage>}
#[derive(Clone,Debug)]
pub struct AutomaticSelection{pub settings:DocumentSettings,pub palette_count:Option<usize>,pub recommended_background_mode:i32}
pub type DocumentStageObserver<'a>=dyn FnMut(&str,&TaskResult,&AutomaticSelection)->Result<(),String>+'a;
#[derive(Clone,Debug)]
pub struct AutomaticResult{pub settings:DocumentSettings,pub palette_count:Option<usize>,pub source_alpha_type:u8,pub recommended_background_mode:i32,pub state:DocumentSnapshot}
pub struct ImageDocument{session:CoreSession,state:DocumentSnapshot,candidates:Vec<Option<AnalyzedPalette>>,shadowing:bool,cluster_colors:bool,shadow_control:Option<i32>,source_alpha_type:Option<u8>,cancel:Arc<AtomicBool>,fitter:Option<ContourFitter>,region_identity:u64,fitter_identity:u64,region_palette:Option<Vec<f32>>,editor:Option<SegmentationEditor>,committed_editing:Option<EditorCheckpoint>,background:BackgroundEditor,pointer:EditorPointer}
impl ImageDocument{
    pub fn new(session:CoreSession)->Self{
        let mut parameters=NUMERIC_DATA["constructor"].clone();parameters["usePalette"]=json!(false);parameters["shadowing"]=json!(true);parameters["clusterColors"]=json!(true);
        let state=DocumentSnapshot{original:OwnedImage::default(),source:OwnedImage::default(),working:OwnedImage::default(),settings:DocumentSettings::default(),classification:DocumentClassification{category:-1,..Default::default()},parameters,stage_keys:StageKeys::default(),completed:[0;25],scores:[1e10;25],palettes:vec![Vec::new();25],segmentation:None,contours:None,smoothing:None,fitting:None,processed:false,status:0,progress:0,cancelled:false,random_state:session.random_state()};
        Self{session,state,candidates:vec![None;25],shadowing:true,cluster_colors:true,shadow_control:None,source_alpha_type:None,cancel:Arc::new(AtomicBool::new(false)),fitter:None,region_identity:0,fitter_identity:0,region_palette:None,editor:None,committed_editing:None,background:BackgroundEditor::default(),pointer:EditorPointer{previous_point:[-1.0,-1.0]}}
    }
    pub fn snapshot(&self)->DocumentSnapshot{self.refresh_snapshot(self.state.clone())}
    /// Stage observers can change shared cancellation or random state without
    /// changing the owned image data. Refresh those fields when reusing a snapshot.
    pub(crate) fn refresh_snapshot(&self,mut state:DocumentSnapshot)->DocumentSnapshot{state.random_state=self.session.random_state();state.cancelled=self.cancel.load(Ordering::Relaxed);state}
    /// Read-only exporter view; unlike snapshot this does not clone raster or geometry storage.
    pub fn state(&self)->&DocumentSnapshot{&self.state}
    /// Explicit saved checkpoint presence, distinct from applied segmentation edits.
    pub fn has_committed_edits(&self)->bool{self.committed_editing.as_ref().is_some_and(|checkpoint|!checkpoint.undo.is_empty())}
    pub fn has_applied_edits(&self)->bool{!self.state.stage_keys.segmentation_edits.is_empty()}
    pub fn editing_changed(&self)->bool{self.editor.as_ref().is_some_and(|editor|!editor.matches_active_checkpoint(self.committed_editing.as_ref()))}
    fn save_editing_checkpoint(&mut self){self.committed_editing=self.editor.as_ref().map(|editor|editor.history_checkpoint())}
    pub fn cancellation_handle(&self)->Arc<AtomicBool>{self.cancel.clone()}
    pub fn request_cancel(&self){self.cancel.store(true,Ordering::Relaxed)}
    pub fn reset(&mut self){let settings=self.state.settings.clone();let mut classification=self.state.classification.clone();classification.category=-1;let session=self.session.clone();let mut replacement=Self::new(session);replacement.state.settings=settings;replacement.state.classification=classification;std::mem::swap(&mut replacement.background,&mut self.background);replacement.background.revert();replacement.background.finish();replacement.pointer.previous_point=self.pointer.previous_point;replacement.cancel=self.cancel.clone();replacement.cancel.store(false,Ordering::Relaxed);*self=replacement}
    pub fn set_source(&mut self,width:usize,height:usize,bgra:&[u8],has_alpha:bool,shadow_control:f64)->Result<(),String>{let original=prepare_original_image(width,height,bgra,has_alpha)?;if !shadow_control.is_finite()||shadow_control.fract()!=0.0||shadow_control<i32::MIN as f64||shadow_control>i32::MAX as f64{return Err("Document source shadowControl must be an explicit signed int32 integer.".into())}self.reset();self.install_source(original,shadow_control as i32);Ok(())}
    fn install_source(&mut self,original:OwnedImage,shadow_control:i32){self.state.source=original.clone();self.state.original=original;self.shadow_control=Some(shadow_control);self.state.parameters["shadowControl"]=json!(shadow_control)}
    /// Replace an oversized source with the native recommended raster. Like a
    /// new source, this drops derived processing/editing state, not controls.
    /// The source-alpha classification is the exception: native classifies the
    /// loaded raster eagerly (document+0x14c) and its shrink wrapper 0x468e30
    /// never clears it, so a binary-alpha source stays binary even when block
    /// averaging produces partial alpha (evidence/native-alpha-cache-20261021).
    pub fn shrink_source(&mut self)->bool{
        let Some(original)=crate::image_resize::shrink_source_image(&self.state.original)else{return false};
        let shadow_control=self.shadow_control.expect("loaded source has shadow control");
        let source_alpha_type=self.source_alpha_type();
        self.reset();self.install_source(original,shadow_control);self.source_alpha_type=Some(source_alpha_type);true
    }
    /// Explicit control preload for source/profile selection, not a processing completion.
    pub fn commit_settings(&mut self,input:&Value)->Result<(),String>{self.state.settings=read_document_settings(input).map_err(|e|e.to_string())?;Ok(())}
    pub fn clear_committed_palette(&mut self){self.state.settings.palette.clear()}
    pub fn cancel_load(&mut self){let status=self.state.status;self.clear_committed_palette();self.reset();self.state.status=status}
    pub fn run_batch_load(&mut self,input:&Value,load:impl FnOnce()->Result<DocumentSource,String>)->Result<(),String>{
        let settings=self.batch_settings(&read_batch_controls(input).map_err(|e|e.to_string())?)?;
        if self.state.original.width!=0{self.state.settings=settings.clone()}
        self.clear_committed_palette();self.reset();
        let result=(||{
            let source=load()?;
            let original=prepare_original_image(source.width,source.height,&source.bgra,source.has_alpha)?;
            let control=source.shadow_control;
            if !control.is_finite()||control.fract()!=0.0||control<i32::MIN as f64||control>i32::MAX as f64{return Err("Document source shadowControl must be an explicit signed int32 integer.".into())}
            self.install_source(original,control as i32);
            self.state.settings=settings;Ok(())
        })();
        if result.is_err(){self.state.status=1}result
    }
    fn begin_task(&mut self){self.cancel.store(false,Ordering::Relaxed);self.state.cancelled=false;self.state.progress=0}
    fn notify(&mut self,event:DocumentProgress,observer:&mut Option<&mut DocumentObserver>)->Result<(),String>{
        self.state.progress=event.percent;
        if let Some(callback)=observer.as_deref_mut(){
            self.state.random_state=self.session.random_state();
            self.state.cancelled=self.cancel.load(Ordering::Relaxed);
            observer_result(&self.cancel,callback(&event,&self.state))?;
        }
        if self.cancel.load(Ordering::Relaxed){return Err(CANCEL_TASK.into())}
        Ok(())
    }
    fn apply_profile(&mut self,settings:&DocumentSettings,mut parameters:Value){if let Some(shadowing)=parameters["shadowing"].as_bool(){self.shadowing=shadowing;self.cluster_colors=parameters["clusterColors"].as_bool().unwrap_or(self.cluster_colors)}parameters["shadowing"]=json!(self.shadowing);parameters["clusterColors"]=json!(self.cluster_colors);if let Some(c)=self.shadow_control{parameters["shadowControl"]=json!(c)}let use_palette=matches!(settings.palette_mode,0|1);parameters["usePalette"]=json!(use_palette);self.state.parameters=parameters;if use_palette{if let Some(segmentation)=self.state.segmentation.as_mut(){segmentation.palette=settings.palette.clone()}}}
    fn prepare_source(&mut self,settings:&DocumentSettings)->Result<(),String>{let original=&self.state.original;let color=settings.background.map(|v|v as f64);let composed=composite_source_image(original.width,original.height,&original.bgra,matches!(settings.background_mode,1|2),&color);let mode=self.state.parameters["Preprocessor::filter_type"].as_f64().unwrap_or(0.0) as u32;let amount=self.state.parameters["Preprocessor::reduce_noise_order"].as_f64().unwrap_or(0.0) as u32;self.state.source=filter_prepared_image(composed,mode,amount)?;self.state.processed=mode!=0;Ok(())}
    fn parameters(settings:&DocumentSettings,stage:&str)->Result<Value,String>{let categories=["photograph","artwork-with-blending","artwork-without-blending"];let levels=["high","medium","low"];if !settings.advanced{return resolve_parameters(&json!({"stage":stage,"mode":"basic","category":categories.get(settings.category as usize).copied().unwrap_or("unclassified"),"quality":levels.get(settings.quality as usize)}))}let mut p=json!({"stage":stage,"mode":"advanced"});match stage{"palette"=>{p["colorSensitivity"]=json!(levels[settings.sensitivity as usize]);p["antiAlias"]=json!(settings.anti_alias)},"segmentation"=>{p["complexity"]=json!(settings.complexity);p["minimumPixels"]=json!(settings.minimum_pixels);p["artifactRejection"]=json!(["none","moderate","aggressive"][settings.artifact_rejection as usize]);p["clusterColors"]=json!(settings.cluster_colors)},_=>{p["smoothness"]=json!(settings.smoothness);p["curveComplexity"]=json!(settings.curve_complexity);p["detectCorners"]=json!(settings.detect_corners);p["antiAlias"]=json!(settings.smoothing_anti_alias)}}resolve_parameters(&p)}
    fn key(&self,stage:&str,settings:&DocumentSettings)->Result<String,String>{Ok(match stage{"palette"=>document_palette_key(settings),"segmentation"=>document_segmentation_key(settings),"smoothing"=>document_smoothing_key(settings,&self.state.stage_keys.segmentation_edits),"fitting"=>document_fitting_key(settings,&self.state.stage_keys.segmentation_edits),_=>return Err("Document task stage must be classification, palette, segmentation, smoothing or fitting.".into())})}
    fn current_key(&self,stage:&str)->&str{match stage{"palette"=>&self.state.stage_keys.palette,"segmentation"=>&self.state.stage_keys.segmentation,"smoothing"=>&self.state.stage_keys.smoothing,_=>&self.state.stage_keys.fitting}}
    fn palette_range(input:&Value)->Result<(usize,usize),String>{let read=|name:&str|input[name].as_f64().filter(|v|v.fract()==0.0&&(2.0..=24.0).contains(v)).map(|v|v as usize).ok_or_else(||format!("Document {name} must be an integer from 2 through 24."));let lower=read("lower")?;Ok((lower,lower.max(read("upper")?)))}
    fn palette_controls(&self)->AnalysisControls{let p=&self.state.parameters;AnalysisControls{width:self.state.working.width,height:self.state.working.height,bins:if p["PaletteFinder::use_16_hist_bins"].as_f64()!=Some(0.0){16}else{8},connected:p["PaletteFinder::use_26_connected_peaks"].as_f64()!=Some(0.0),anti_alias:p["Shared::is_anti_aliased"].as_f64()!=Some(0.0),cost_per_color:p["PaletteFinder::cost_per_color"].as_f64().unwrap_or(f64::NAN),edge_scale:p["EdgeImage::edge_weight_scale"].as_f64().unwrap_or(f64::NAN),edge_floor:p["EdgeImage::edge_weight_floor"].as_f64().unwrap_or(f64::NAN)}}
    fn finish_task(&mut self,settings:DocumentSettings){self.state.status=0;self.state.progress=100;self.state.settings=settings;self.save_editing_checkpoint()}
    /// Classification and vector continuations check cancellation after progress 100,
    /// before copying document-owned settings and both editing histories.
    fn finish_cancellable_task(&mut self,settings:DocumentSettings)->bool{
        self.state.progress=100;
        if self.cancel.load(Ordering::Relaxed){self.state.status=2;false}
        else{self.finish_task(settings);true}
    }
    pub fn analyze(&mut self,input:&Value,observer:Option<&mut DocumentObserver>)->Result<DocumentSnapshot,String>{let settings=read_document_settings(input).map_err(|e|e.to_string())?;let (lower,last)=Self::palette_range(input)?;self.analyze_settings(settings,lower,last,observer)?;Ok(self.snapshot())}
    fn analyze_settings(&mut self,settings:DocumentSettings,lower:usize,last:usize,mut observer:Option<&mut DocumentObserver>)->Result<(),String>{let parameters=Self::parameters(&settings,"palette")?;if parameters["Shared::is_anti_aliased"].as_f64()!=Some(0.0)&&(self.state.original.width<2||self.state.original.height<2){return Err("Anti-alias document analysis needs dimensions of at least 2 by 2.".into())}self.begin_task();self.apply_profile(&settings,parameters);let key=document_palette_key(&settings);let changed=key!=self.state.stage_keys.palette;if changed{self.state.completed.fill(0)}self.state.stage_keys.palette=key;let work=changed||(lower..=last).any(|n|self.state.completed[n]==0);
        if work{self.prepare_source(&settings)?;let source=&self.state.source;self.state.working=reduce_working_image(source.width,source.height,&source.bgra,self.state.parameters["Shared::is_anti_aliased"].as_f64()!=Some(0.0),true);let prepared=PreparedPaletteAnalysis::from_prepared(&self.state.working.bgra,self.palette_controls());let session=self.session.clone();let pixels=self.state.working.bgra.clone();
            for count in lower..=last{if self.state.completed[count]!=0{continue}let candidate=prepared.evaluate(&session,&pixels,count,Some(&mut|iteration,palette,indices|{let width=self.state.working.width;let height=self.state.working.height;let map=self.state.segmentation.get_or_insert_with(||SegmentationSnapshot{width,height,indices:None,palette:Vec::new(),counts:Vec::new(),color_indices:Vec::new()});if map.width!=width||map.height!=height{map.indices=None}map.width=width;map.height=height;map.palette=palette.to_vec();if let Some(indices)=indices{map.indices=Some(indices.to_vec())}let fraction=((iteration as f64*5.0*0.04).min(1.0)+(count-lower) as f64)*(1.0/(last-lower+1) as f64);self.notify(DocumentProgress{stage:"palette",phase:0,substep:0,counter:iteration as i32,fraction,percent:(fraction*100.0+0.5).trunc() as i32,color_count:Some(count),iteration:Some(iteration)},&mut observer)}))?;
                let map=self.state.segmentation.as_mut().expect("refinement publishes live map");map.indices=Some(candidate.generated.indices.clone());map.palette=candidate.generated.palette.clone();self.state.palettes[count]=candidate.generated.palette.chunks_exact(4).flat_map(|c|c.iter().rev().copied()).collect();self.state.scores[count]=candidate.score.total;self.state.completed[count]=1;self.candidates[count]=Some(candidate);
            }
            if lower==2&&last==12{let candidates:Vec<_>=self.candidates[2..13].iter().map(|c|c.as_ref().expect("completed palette").clone()).collect();let (_,classification)=prepared.classify(&pixels,&candidates)?;let mut minimum=lower;let mut minimum_score=self.state.scores[lower];for count in lower..25{if self.state.scores[count]<minimum_score{minimum=count;minimum_score=self.state.scores[count]}self.state.scores[count]=1.0}if classification.mode==2{self.state.scores[minimum]=0.75}else{self.state.scores[classification.color_count as usize]=0.0}}
        }self.finish_task(settings);Ok(())
    }
    pub fn classify(&mut self,input:&Value)->Result<TaskResult,String>{let settings=read_document_settings(input).map_err(|e|e.to_string())?;let extension=input["extension"].as_str().ok_or("Document classification requires an explicit extension string.")?;if self.state.original.width<2||self.state.original.height<2{return Err("Document classification needs dimensions of at least 2 by 2.".into())}let parameters=resolve_parameters(&json!({"stage":"classification"}))?;self.begin_task();self.apply_profile(&DocumentSettings{palette_mode:2,..settings.clone()},parameters);self.prepare_source(&settings)?;self.state.stage_keys.segmentation.clear();let source=self.state.source.clone();let image=ImageState{working:source.clone(),source};let analysis=analyze_prepared_image(&self.session,image,&self.state.parameters,extension,self.palette_controls())?;self.state.working=analysis.image.working.clone();self.state.parameters=analysis.parameters.clone();
        // Classification publishes candidate pixels before region segmentation.
        // Existing region metadata has a separate lifetime and must survive.
        let candidate=analysis.palette_analysis.candidates.last().unwrap();
        let (width,height)=(self.state.working.width,self.state.working.height);
        let map=self.state.segmentation.get_or_insert_with(||SegmentationSnapshot{width,height,indices:None,palette:Vec::new(),counts:Vec::new(),color_indices:Vec::new()});
        map.width=width;map.height=height;map.indices=Some(candidate.generated.indices.clone());map.palette=candidate.generated.palette.clone();
        let c=analysis.image_analysis.classification;let p=analysis.palette_analysis.classification;self.state.classification=DocumentClassification{category:c.image_type as i32,quality:c.quality as i32,palette_mode:p.mode as i32,color_count:p.color_count as i32};self.finish_cancellable_task(settings.clone());Ok(TaskResult{did_work:true,settings,state:self.snapshot(),analysis:Some(analysis)})
    }
    pub fn segment(&mut self,input:&Value,observer:Option<&mut DocumentObserver>)->Result<DocumentSnapshot,String>{let settings=read_document_settings(input).map_err(|e|e.to_string())?;self.segment_settings(settings,observer)?;Ok(self.snapshot())}
    fn segment_settings(&mut self,settings:DocumentSettings,mut observer:Option<&mut DocumentObserver>)->Result<(),String>{if matches!(settings.palette_mode,0|1)&&settings.palette.is_empty(){return Err("Fixed document segmentation needs at least one palette color.".into())}let parameters=Self::parameters(&settings,"segmentation")?;if parameters["Shared::is_anti_aliased"].as_f64()!=Some(0.0)&&(self.state.original.width<2||self.state.original.height<2){return Err("Anti-alias document segmentation needs dimensions of at least 2 by 2.".into())}self.begin_task();self.apply_profile(&settings,parameters);let key=document_segmentation_key(&settings);let work=key!=self.state.stage_keys.segmentation;self.state.stage_keys.segmentation=key;
        if work{self.prepare_source(&settings)?;self.state.working=self.state.source.clone();let pixels=self.state.working.clone();let palette=matches!(settings.palette_mode,0|1).then_some(settings.palette.as_slice());let mut owner=SegmentationState::new(self.state.segmentation.as_ref(),pixels.width,pixels.height,palette);self.region_palette=None;if let Some(f)=self.state.fitting.as_mut(){f.complete=false}let mut parameters=self.state.parameters.clone();let result=owner.run(Raster{width:pixels.width,height:pixels.height,bgra:&pixels.bgra},&mut parameters,palette,&mut|event,live|{self.state.segmentation=Some(live.clone());if let Some(minimum)=event.minimum_pixels{self.state.parameters["Segmenter::min_num_pixels"]=json!(minimum)}self.notify(DocumentProgress{stage:"segmentation",phase:event.phase,substep:event.substep,counter:event.counter,fraction:event.fraction,percent:event.percent,color_count:None,iteration:None},&mut observer)}).map(|_|());self.state.parameters=parameters;
            if let Err(error)=result{self.state.stage_keys.segmentation.clear();self.state.stage_keys.segmentation_edits.clear();self.state.stage_keys.smoothing.clear();self.state.stage_keys.fitting.clear();return self.interrupted(error)}
            let segmented=owner.into_snapshot();self.region_palette=Some(segmented.palette.clone());self.state.segmentation=Some(segmented);self.region_identity=self.region_identity.wrapping_add(1);self.editor=None;self.committed_editing=None;self.state.contours=None;self.state.smoothing=None;self.state.fitting=None;self.state.stage_keys.segmentation_edits.clear();self.state.stage_keys.smoothing.clear();self.state.stage_keys.fitting.clear();
        }self.finish_task(settings);Ok(())
    }
    fn interrupted(&mut self,error:String)->Result<(),String>{if error!=CANCEL_TASK{return Err(error)}self.cancel.store(true,Ordering::Relaxed);self.state.status=2;self.state.progress=100;Ok(())}
    fn bind_smoothing(&mut self)->Result<(),String>{let geometry=self.state.contours.as_ref().ok_or("Document fitting needs completed contours.")?;match self.fitter.as_mut(){None=>self.fitter=Some(ContourFitter::new(geometry)?),Some(f) if self.fitter_identity==self.region_identity=>f.set_contours(geometry)?,Some(f)=>f.set_regions(geometry)?}self.fitter_identity=self.region_identity;Ok(())}
    pub fn smooth(&mut self,input:&Value,observer:Option<&mut DocumentObserver>)->Result<DocumentSnapshot,String>{let settings=read_document_settings(input).map_err(|e|e.to_string())?;self.smooth_settings(settings,observer)?;Ok(self.snapshot())}
    fn smooth_settings(&mut self,settings:DocumentSettings,mut observer:Option<&mut DocumentObserver>)->Result<(),String>{if self.region_palette.is_none(){return Err("Document smoothing needs completed segmentation.".into())}let parameters=Self::parameters(&settings,"smoothing")?;self.begin_task();self.apply_profile(&settings,parameters);let key=document_smoothing_key(&settings,&self.state.stage_keys.segmentation_edits);let work=key!=self.state.stage_keys.smoothing;self.state.stage_keys.smoothing=key;
        if work{let map=self.state.segmentation.as_ref().unwrap();let indices:Vec<i64>=map.indices.as_ref().ok_or("Document smoothing needs produced labels.")?.iter().map(|v|*v as i64).collect();let counts:Vec<i64>=map.counts.iter().map(|v|*v as i64).collect();let color_indices:Vec<i64>=map.color_indices.iter().map(|v|*v as i64).collect();let palette:Vec<f64>=self.region_palette.as_ref().unwrap().iter().map(|v|*v as f64).collect();let geometry=extract_contours(&ContourInput{width:map.width as f64,height:map.height as f64,indices:&indices,counts:&counts,color_indices:&color_indices,palette:&palette,anti_alias:Some(self.state.parameters["Shared::is_anti_aliased"].as_f64()!=Some(0.0))}).map_err(|e|e.to_string())?;let live_palette:Vec<f64>=map.palette.iter().map(|v|*v as f64).collect();let pixels=self.state.working.clone();let parameters=self.state.parameters.clone();self.state.fitting=None;let session=self.session.clone();let result=smooth_contour_geometry(&SmoothContourInput{width:pixels.width as i64,height:pixels.height as i64,bgra:&pixels.bgra,geometry:&geometry,indices:&indices,palette:&live_palette,parameters:&parameters},&mut||session.next_random(),Some(&mut|event,live|{self.state.contours=Some(live.geometry.clone());self.state.smoothing=Some(SmoothedContours{geometry:live.geometry.clone(),penalty_counts:live.penalty_counts.to_vec(),border_counts:live.border_counts,reference_positions:live.reference_positions.to_vec()});self.notify(DocumentProgress{stage:"smoothing",phase:event.phase,substep:event.substep,counter:event.counter,fraction:event.fraction,percent:event.percent,color_count:None,iteration:None},&mut observer)}));
            match result{Ok(smoothed)=>{self.state.contours=Some(smoothed.geometry.clone());self.state.smoothing=Some(smoothed)},Err(e)=>{self.state.stage_keys.smoothing.clear();self.state.stage_keys.fitting.clear();if self.state.contours.is_some(){self.bind_smoothing()?}return self.interrupted(e)}}self.state.fitting=None;self.state.stage_keys.fitting.clear();self.bind_smoothing()?;
        }
        if !self.finish_cancellable_task(settings){self.state.stage_keys.smoothing.clear();self.state.stage_keys.fitting.clear()}
        Ok(())
    }
    fn sync_fitting(&mut self,snapshot:FitterSnapshot){if let Some(contours)=self.state.contours.as_mut(){for (n,f) in contours.nodes.iter_mut().zip(&snapshot.nodes){n.kind=f.kind}}let chains=self.state.fitting.as_mut().and_then(|f|f.output_chains.take());self.state.fitting=Some(DocumentFitting{complete:false,geometry:FittedGeometry{cubic_storage_slots:snapshot.cubic_storage_slots,cubics:snapshot.cubics,nodes:snapshot.nodes,regions:snapshot.regions},cubic_availability:snapshot.cubic_availability,output_chains:chains})}
    pub fn fit(&mut self,input:&Value,observer:Option<&mut DocumentObserver>)->Result<DocumentSnapshot,String>{let settings=read_document_settings(input).map_err(|e|e.to_string())?;self.fit_settings(settings,observer)?;Ok(self.snapshot())}
    fn fit_settings(&mut self,settings:DocumentSettings,mut observer:Option<&mut DocumentObserver>)->Result<(),String>{if self.state.contours.is_none(){return Err("Document fitting needs completed contours.".into())}let parameters=Self::parameters(&settings,"fitting")?;self.begin_task();self.apply_profile(&settings,parameters);let key=document_fitting_key(&settings,&self.state.stage_keys.segmentation_edits);let work=key!=self.state.stage_keys.fitting;self.state.stage_keys.fitting=key;
        if work{let mut fitter=self.fitter.take().ok_or("Document fitting needs completed contours.")?;let parameters=FittingParameters::from_json(&self.state.parameters);let result=fitter.run(&parameters,Some(&mut|event,live:&ContourFitter|{if observer.is_some(){self.sync_fitting(live.snapshot())}self.notify(DocumentProgress{stage:"fitting",phase:event.phase,substep:event.substep,counter:event.counter,fraction:event.fraction,percent:event.percent,color_count:None,iteration:None},&mut observer)}));match result{Ok(geometry)=>{if let Some(contours)=self.state.contours.as_mut(){for (n,f) in contours.nodes.iter_mut().zip(&geometry.nodes){n.kind=f.kind}}let output_chains=Some(assemble_output_shapes(&geometry));self.background.accept_vectors(geometry.regions.len());self.state.fitting=Some(DocumentFitting{complete:true,geometry,cubic_availability:fitter.cubic_availability(),output_chains});self.fitter=Some(fitter)},Err(error)=>{self.sync_fitting(fitter.snapshot());self.fitter=Some(fitter);self.state.stage_keys.fitting.clear();return self.interrupted(error)}}
        }
        if !self.finish_cancellable_task(settings){self.state.stage_keys.fitting.clear()}
        Ok(())
    }
    pub fn run_task(&mut self,stage:&str,input:&Value,observer:Option<&mut DocumentObserver>)->Result<TaskResult,String>{if stage=="classification"{return self.classify(input)}let settings=read_document_settings(input).map_err(|e|e.to_string())?;let mut work=self.key(stage,&settings)?!=self.current_key(stage);let range=if stage=="palette"{Some(Self::palette_range(input)?)}else{None};if let Some((lower,last))=range{work|=(lower..=last).any(|n|self.state.completed[n]==0)}if !work{if stage=="palette"{self.state.settings=settings.clone();self.save_editing_checkpoint()}}else{match stage{"palette"=>{let (lower,last)=range.unwrap();self.analyze_settings(settings.clone(),lower,last,observer)?},"segmentation"=>self.segment_settings(settings.clone(),observer)?,"smoothing"=>self.smooth_settings(settings.clone(),observer)?,_=>self.fit_settings(settings.clone(),observer)?}}Ok(TaskResult{did_work:work,settings,state:self.snapshot(),analysis:None})}
    pub fn source_alpha_type(&mut self)->u8{*self.source_alpha_type.get_or_insert_with(||{let mut kind=0;for p in self.state.original.bgra.chunks_exact(4){if p[3]>0&&p[3]<255{return 2}if p[3]==0{kind=1}}kind})}
    pub fn run_export(&mut self,save:impl FnOnce(&mut dyn FnMut(i32)->Result<(),String>)->Result<u32,String>)->Result<DocumentSnapshot,String>{
        if !self.state.fitting.as_ref().is_some_and(|f|f.complete){return Err("Document export needs completed fitted geometry.".into())}
        self.begin_task();
        let result=save(&mut|percent|{self.state.progress=percent;if self.cancel.load(Ordering::Relaxed){Err(CANCEL_TASK.into())}else{Ok(())}});
        self.state.status=observer_result(&self.cancel,result)?;
        Ok(self.snapshot())
    }
    /// Application save path borrowing fitted storage instead of snapshotting it.
    /// The saver retains native open/flush/interruption status and file lifetime.
    pub fn save_vector_file(&mut self,path:&std::path::Path,options:&crate::export_options::ExportOptions,on_progress:Option<&mut dyn FnMut(i32)->Result<(),String>>)->Result<crate::vector_file::VectorFileResult,String>{
        self.save_vector_file_with_environment(path,options,&DocumentExportEnvironment::default(),on_progress)
    }
    pub fn save_vector_file_with_environment(&mut self,path:&std::path::Path,options:&crate::export_options::ExportOptions,environment:&DocumentExportEnvironment,on_progress:Option<&mut dyn FnMut(i32)->Result<(),String>>)->Result<crate::vector_file::VectorFileResult,String>{
        self.save_vector_file_admitted(path,options,environment,true,on_progress)
    }
    pub fn save_vector_file_admitted(&mut self,path:&std::path::Path,options:&crate::export_options::ExportOptions,environment:&DocumentExportEnvironment,overwrite:bool,on_progress:Option<&mut dyn FnMut(i32)->Result<(),String>>)->Result<crate::vector_file::VectorFileResult,String>{
        self.save_vector_file_impl(path,options,environment,overwrite,false,on_progress)
    }
    pub fn save_vector_file_safely(&mut self,path:&std::path::Path,options:&crate::export_options::ExportOptions,on_progress:Option<&mut dyn FnMut(i32)->Result<(),String>>)->Result<crate::vector_file::VectorFileResult,String>{
        self.save_vector_file_impl(path,options,&DocumentExportEnvironment::default(),false,true,on_progress)
    }
    fn save_vector_file_impl(&mut self,path:&std::path::Path,options:&crate::export_options::ExportOptions,environment:&DocumentExportEnvironment,overwrite:bool,safe:bool,mut on_progress:Option<&mut dyn FnMut(i32)->Result<(),String>>)->Result<crate::vector_file::VectorFileResult,String>{
        if !self.state.fitting.as_ref().is_some_and(|f|f.complete){return Err("Document export needs completed fitted geometry.".into())}
        self.begin_task();
        let hidden:Vec<i64>=self.background.snapshot().hidden_regions.iter().map(|v|*v as i64).collect();
        let geometry=&self.state.fitting.as_ref().unwrap().geometry;
        let vector=crate::vector_output::VectorOutputInput{geometry,width:self.state.source.width as i64,height:self.state.source.height as i64,output_size:environment.output_size,layout:options.layout,thin_stroke:options.thin_stroke,hidden_regions:&hidden};
        let mut input=crate::vector_file::VectorFileInput::new(vector);
        input.eps.compact=options.compact;
        input.eps.creation_date=environment.creation_date.clone();
        input.reference_device=environment.reference_device;
        if input.reference_device.is_none()&&path.as_os_str().as_encoded_bytes().ends_with(b".emf"){
            input.reference_device=Some(crate::emf::EmfDevice::current_display().map_err(|e|e.to_string())?);
        }
        input.palette=self.state.segmentation.as_ref().map_or(&[][..],|s|s.palette.as_slice());
        input.dxf_curve_mode=match options.curve_mode{crate::export_options::CurveMode::Splines=>crate::dxf::DxfCurveMode::Splines,crate::export_options::CurveMode::SparseLines=>crate::dxf::DxfCurveMode::SparseLines,crate::export_options::CurveMode::DenseLines=>crate::dxf::DxfCurveMode::DenseLines};
        let progress=&mut self.state.progress;let cancel=&self.cancel;
        let mut observer=|percent|{
            *progress=percent;
            if let Some(cb)=on_progress.as_deref_mut(){observer_result(cancel,cb(percent)).map_err(crate::vector_output::VectorOutputError)?}
            if cancel.load(Ordering::Relaxed){return Err(crate::vector_output::VectorOutputError(CANCEL_TASK.into()))}
            Ok(())
        };
        let result=if safe { crate::vector_file::write_vector_file_safely(path,&input,&mut observer) } else { crate::vector_file::write_vector_file_admitted(path,&input,overwrite,&mut observer) }.map_err(|e|e.to_string())?;
        self.state.status=result.status as u32;Ok(result)
    }
    fn merged_settings(&self,input:&Value)->Result<DocumentSettings,String>{let mut s=self.state.settings.to_json();let values=input.as_object().ok_or("Automatic processing requires controls with an extension.")?;for(k,v)in values{s.insert(k.clone(),v.clone());}read_document_settings(&Value::Object(s)).map_err(|e|e.to_string())}
    fn batch_settings(&self,controls:&BatchControls)->Result<DocumentSettings,String>{let mut s=self.state.settings.to_json();for (k,v) in &controls.overrides{s.insert(k.clone(),v.clone());}for(k,v)in [("advanced",json!(controls.advanced)),("category",json!(controls.category)),("quality",json!(controls.quality)),("paletteMode",json!(controls.palette_mode)),("backgroundMode",json!(controls.background_mode)),("palette",json!(controls.palette)),("background",json!([1,1,1,1]))]{s.insert(k.into(),v);}read_document_settings(&Value::Object(s)).map_err(|e|e.to_string())}
    fn selected_palette(&self,count:usize)->Vec<f32>{self.state.palettes[count].chunks_exact(4).flat_map(|c|c.iter().rev().map(|v|(((*v as f64*65535.0+0.5).floor())/65535.0) as f32)).collect()}
    fn minimum_palette(&self)->Option<usize>{let mut minimum=1e10;let mut selected=None;for count in 0..25{if self.state.completed[count]!=0&&(self.state.scores[count] as f64)<minimum{minimum=self.state.scores[count] as f64;selected=Some(count)}}selected}
    pub fn run_automatic(&mut self,input:&Value)->Result<AutomaticResult,String>{self.run_automatic_with_observers(input,None,None)}
    pub fn run_automatic_with_observer(&mut self,input:&Value,observer:Option<&mut DocumentObserver>)->Result<AutomaticResult,String>{self.run_automatic_with_observers(input,observer,None)}
    pub fn run_automatic_with_observers(&mut self,input:&Value,mut observer:Option<&mut DocumentObserver>,mut on_stage:Option<&mut DocumentStageObserver>)->Result<AutomaticResult,String>{
        let extension=input["extension"].as_str().ok_or("Automatic processing requires an extension string.")?;let mut settings=self.merged_settings(input)?;settings.advanced=false;let alpha=self.source_alpha_type();let mut classify=Value::Object(settings.to_json());classify["extension"]=json!(extension);let result=self.run_task("classification",&classify,None)?;if result.state.status==2{return Err(CANCEL_TASK.into())}let c=&result.state.classification;let recommended=i32::from(c.category==1&&alpha==1);settings.category=c.category;settings.quality=c.quality;settings.palette_mode=c.palette_mode;settings.background_mode=recommended;settings.background=[1.0,1.0,1.0,recommended as f32];let mut selection=AutomaticSelection{settings,palette_count:None,recommended_background_mode:recommended};if let Some(cb)=on_stage.as_deref_mut(){cb("classification",&result,&selection)?}
        if matches!(selection.settings.palette_mode,0|1){let mut controls=Value::Object(selection.settings.to_json());controls["lower"]=json!(2);controls["upper"]=json!(12);let result=self.run_task("palette",&controls,observer.as_mut().map(|o|&mut **o as &mut DocumentObserver))?;let count=self.minimum_palette().ok_or("Automatic palette selection found no eligible candidate.")?;selection.palette_count=Some(count);selection.settings.palette=self.selected_palette(count);if let Some(cb)=on_stage.as_deref_mut(){cb("palette",&result,&selection)?}}
        for stage in ["segmentation","smoothing","fitting"]{let result=self.run_task(stage,&Value::Object(selection.settings.to_json()),observer.as_mut().map(|o|&mut **o as &mut DocumentObserver))?;if result.did_work&&result.state.status==2{return Err(CANCEL_TASK.into())}if let Some(cb)=on_stage.as_deref_mut(){cb(stage,&result,&selection)?}}
        Ok(AutomaticResult{settings:selection.settings,palette_count:selection.palette_count,source_alpha_type:alpha,recommended_background_mode:recommended,state:self.snapshot()})
    }
    pub fn run_batch_image(&mut self,input:&Value)->Result<AutomaticResult,String>{self.run_batch_image_with_observers(input,None,None)}
    pub fn run_batch_image_with_observers(&mut self,input:&Value,mut observer:Option<&mut DocumentObserver>,mut on_stage:Option<&mut DocumentStageObserver>)->Result<AutomaticResult,String>{
        let requested=read_batch_controls(input).map_err(|e|e.to_string())?;let extension=input["extension"].as_str().ok_or("Batch image processing requires an extension string.")?;let mut settings=self.batch_settings(&requested)?;let alpha=self.source_alpha_type();let classify=!requested.advanced&&[requested.category,requested.quality,requested.palette_mode].contains(&3);
        if classify{let mut controls=Value::Object(settings.to_json());controls["extension"]=json!(extension);let result=self.run_task("classification",&controls,None)?;if result.state.status==2{return Err(CANCEL_TASK.into())}if let Some(cb)=on_stage.as_deref_mut(){cb("classification",&result,&AutomaticSelection{settings:settings.clone(),palette_count:None,recommended_background_mode:0})?}}
        let c=&self.state.classification;if classify&&requested.category==3{settings.category=c.category}if classify&&requested.quality==3{settings.quality=c.quality}if classify&&requested.palette_mode==3{settings.palette_mode=c.palette_mode}let recommended=i32::from(settings.category==1&&alpha==1);if !requested.advanced&&requested.background_mode==3{settings.background_mode=recommended;settings.background=[1.0,1.0,1.0,recommended as f32]}
        let mut selection=AutomaticSelection{settings,palette_count:None,recommended_background_mode:recommended};
        if if requested.advanced{requested.palette_mode==1}else{requested.palette_mode!=2}{let mut controls=Value::Object(selection.settings.to_json());controls["lower"]=json!(if requested.advanced{requested.color_count}else{2});controls["upper"]=json!(if requested.advanced{requested.color_count}else{12});let result=self.run_task("palette",&controls,observer.as_mut().map(|o|&mut **o as &mut DocumentObserver))?;if let Some(cb)=on_stage.as_deref_mut(){cb("palette",&result,&selection)?}let count=if requested.advanced{Some(requested.color_count as usize)}else if selection.settings.palette_mode==0{Some(2)}else if requested.palette_mode==1&&requested.color_count!=0{Some(requested.color_count as usize)}else{self.minimum_palette()}.ok_or("Batch palette selection found no eligible candidate.")?;selection.palette_count=Some(count);selection.settings.palette=self.selected_palette(count)}
        for stage in ["segmentation","smoothing","fitting"]{let result=self.run_task(stage,&Value::Object(selection.settings.to_json()),observer.as_mut().map(|o|&mut **o as &mut DocumentObserver))?;if result.state.status==2{return Err(CANCEL_TASK.into())}if let Some(cb)=on_stage.as_deref_mut(){cb(stage,&result,&selection)?}}
        Ok(AutomaticResult{settings:selection.settings,palette_count:selection.palette_count,source_alpha_type:alpha,recommended_background_mode:recommended,state:self.snapshot()})
    }
    pub fn begin_editing(&mut self)->Result<EditorSnapshot,String>{
        let map=self.state.segmentation.as_ref().ok_or("Document editing needs completed segmentation.")?;let palette=self.region_palette.as_ref().ok_or("Document editing needs completed segmentation.")?;self.background.finish();
        if self.editor.is_none(){let indices=map.indices.as_ref().ok_or("Document editing needs completed segmentation.")?;let mut bgra=vec![0;map.width*map.height*4];for(pixel,&index)in indices.iter().enumerate(){let color=map.color_indices[index as usize] as usize;for c in 0..4{bgra[pixel*4+c]=(palette[color*4+c] as f64*255.0+0.5).floor() as u8}}let background=self.state.settings.background.map(|v|v as f64);let color:Vec<f64>=palette[..4].iter().map(|v|*v as f64).collect();self.editor=Some(SegmentationEditor::new(map.width as i64,map.height as i64,&bgra,&self.state.original.bgra,&background,&color,self.state.settings.background_mode)?)}Ok(self.editor.as_ref().unwrap().snapshot())
    }
    fn editing_session(&mut self)->Result<&mut SegmentationEditor,String>{self.editor.as_mut().ok_or_else(||"Call beginEditing before editing the document.".into())}
    pub fn edit(&mut self,command:&Value)->Result<EditorSnapshot,String>{self.editing_session()?.apply(command)}
    pub fn editing_state(&self)->Option<EditorSnapshot>{self.editor.as_ref().map(|e|e.snapshot())}
    pub fn editing_history(&self)->(usize,usize){self.editor.as_ref().map_or((0,0),|e|e.history_state())}
    pub fn leave_editing(&mut self)->Result<(),String>{self.editing_session()?.end_editing();Ok(())}
    pub fn restore_committed_editing(&mut self)->Result<Option<EditorSnapshot>,String>{match self.editor.as_mut(){None=>Ok(None),Some(editor)=>{let empty=EditorCheckpoint{undo:Vec::new(),redo:Vec::new()};Ok(Some(editor.restore_checkpoint(self.committed_editing.as_ref().unwrap_or(&empty))?))}}}
    pub fn gesture(&mut self,event:&Value)->Result<EditorSnapshot,String>{self.editor.as_mut().ok_or("Call beginEditing before editing the document.")?.gesture(event,&mut self.pointer)}
    pub fn grouped_gesture(&mut self,event:&Value)->Result<EditorSnapshot,String>{self.editor.as_mut().ok_or("Call beginEditing before editing the document.")?.grouped_gesture(event,&mut self.pointer)}
    pub fn grouped_undo_edit(&mut self)->Result<EditorSnapshot,String>{self.editing_session()?.grouped_undo()}
    pub fn grouped_redo_edit(&mut self)->Result<EditorSnapshot,String>{self.editing_session()?.grouped_redo()}
    pub fn undo_edit(&mut self)->Result<EditorSnapshot,String>{self.editing_session()?.undo()}
    pub fn redo_edit(&mut self)->Result<EditorSnapshot,String>{self.editing_session()?.redo()}
    pub fn revert_edits(&mut self)->Result<EditorSnapshot,String>{self.editing_session()?.revert()}
    pub fn background_state(&self)->BackgroundSnapshot{self.background.snapshot()}
    pub fn begin_background_editing(&mut self)->Result<BackgroundSnapshot,String>{let background=self.background.begin()?;if let Some(editor)=self.editor.as_mut(){editor.end_editing()}Ok(background)}
    pub fn toggle_background(&mut self,point:[f64;2])->Result<BackgroundSnapshot,String>{let map=self.state.segmentation.as_ref();self.background.toggle(point,map.map_or(0,|m|m.width as i32),map.map_or(0,|m|m.height as i32),map.and_then(|m|m.indices.as_deref()).unwrap_or(&[]),self.state.contours.as_ref())}
    pub fn background_gesture(&mut self,event:&Value)->Result<BackgroundSnapshot,String>{let map=self.state.segmentation.as_ref();self.background.gesture(event,map.map_or(0,|m|m.width as i32),map.map_or(0,|m|m.height as i32),map.and_then(|m|m.indices.as_deref()).unwrap_or(&[]),self.state.contours.as_ref(),&mut self.pointer)}
    pub fn undo_background(&mut self)->BackgroundSnapshot{self.background.undo()}
    pub fn redo_background(&mut self)->BackgroundSnapshot{self.background.redo()}
    pub fn revert_background(&mut self)->BackgroundSnapshot{self.background.revert()}
    pub fn finish_background_editing(&mut self)->BackgroundSnapshot{self.background.finish()}
    /// Explicit API commit retains its checkpoint-before-processing semantics.
    pub fn commit_editing(&mut self)->Result<DocumentSnapshot,String>{
        self.editing_session()?;
        self.save_editing_checkpoint();
        self.apply_editing()
    }
    /// Advanced Next applies pixels without advancing the document Back checkpoint.
    pub fn apply_editing(&mut self)->Result<DocumentSnapshot,String>{
        let editor=self.editing_session()?.snapshot();self.editor.as_mut().unwrap().end_editing();self.background.finish();
        if editor.key!=self.state.stage_keys.segmentation_edits{let colors:Vec<[u8;4]>=editor.bgra.chunks_exact(4).map(|c|[c[0],c[1],c[2],c[3]]).collect();let (palette,labels)=crate::region_data::palette_from_byte_colors(&colors);let previous:Vec<_>=(0..palette.len()/4).map(|i|crate::region_data::RegionStatistic{count:0,sums:[0.0;4],color_index:i as i32}).collect();let(indices,statistics)=crate::region_data::rebuild_connected_regions(editor.width,&editor.bgra,&labels,Some(&previous));self.state.segmentation=Some(SegmentationSnapshot{width:editor.width,height:editor.height,indices:Some(indices),palette:palette.clone(),counts:statistics.iter().map(|r|r.count as i32).collect(),color_indices:statistics.iter().map(|r|r.color_index).collect()});self.region_palette=Some(palette);self.region_identity=self.region_identity.wrapping_add(1);self.state.stage_keys.segmentation_edits=editor.key;self.state.contours=None;self.state.smoothing=None;self.state.fitting=None}Ok(self.snapshot())
    }
    pub fn finish_editing(&mut self,input:&Value,observer:Option<&mut DocumentObserver>)->Result<DocumentSnapshot,String>{self.finish_editing_with_observers(input,observer,None)}
    pub fn finish_editing_with_observers(&mut self,input:&Value,mut observer:Option<&mut DocumentObserver>,mut on_stage:Option<&mut dyn FnMut(&str,&TaskResult)->Result<(),String>>)->Result<DocumentSnapshot,String>{
        let settings=read_document_settings(input).map_err(|e|e.to_string())?;self.commit_editing()?;
        for stage in ["smoothing","fitting"]{
            let result=self.run_task(stage,&Value::Object(settings.to_json()),observer.as_mut().map(|o|&mut **o as &mut DocumentObserver))?;
            if result.did_work&&result.state.status==2{return Err(CANCEL_TASK.into())}
            if let Some(cb)=on_stage.as_deref_mut(){cb(stage,&result)?}
        }
        Ok(self.snapshot())
    }
}

impl DocumentSnapshot{
    pub fn to_json(&self)->Value{
        let raster=|r:&OwnedImage|json!({"width":r.width,"height":r.height,"bgra":r.bgra});
        let mut original=raster(&self.original);
        let mut source=raster(&self.source);
        if self.original.width==0{original["matte"]=json!(0);source["matte"]=json!(0)}
        let segmentation=self.segmentation.as_ref().map(|m|json!({"width":m.width,"height":m.height,"indices":m.indices,"palette":m.palette,"counts":m.counts,"colorIndices":m.color_indices}));
        let smoothing=self.smoothing.as_ref().map(|s|json!({"borderCounts":s.border_counts,"referencePositions":s.reference_positions,"penaltyCounts":s.penalty_counts}));
        let fitting=self.fitting.as_ref().map(|f|json!({"complete":f.complete,"geometry":f.geometry.to_json(),"cubicAvailability":f.cubic_availability,"outputChains":f.output_chains.as_ref().map(|c|json!({"children":c.children,"holes":c.holes}))}));
        json!({"original":original,"source":source,"working":raster(&self.working),"settings":self.settings.to_json(),"classification":{"category":self.classification.category,"quality":self.classification.quality,"paletteMode":self.classification.palette_mode,"colorCount":self.classification.color_count},"parameters":self.parameters,"stageKeys":{"palette":self.stage_keys.palette,"reservedC0":self.stage_keys.reserved_c0,"segmentation":self.stage_keys.segmentation,"segmentationEdits":self.stage_keys.segmentation_edits,"smoothing":self.stage_keys.smoothing,"fitting":self.stage_keys.fitting},"completed":self.completed,"scores":self.scores,"palettes":self.palettes,"segmentation":segmentation,"contours":self.contours.as_ref().map(|c|c.to_json()),"smoothing":smoothing,"fitting":fitting,"processed":self.processed,"status":self.status,"progress":self.progress,"cancelled":self.cancelled,"randomState":self.random_state})
    }
}

#[cfg(test)]
mod checkpoint_tests{
    use super::*;
    #[test]
    fn cancelled_final_copy_preserves_settings_and_both_histories(){
        let pixels=vec![255;16];
        let mut document=CoreSession::default().create_document();
        document.editor=Some(SegmentationEditor::new(2,2,&pixels,&pixels,&[0.0,0.0,0.0,1.0],&[0.0,0.0,0.0,1.0],0).unwrap());
        document.edit(&json!({"tool":"fill","x":0,"y":0,"color":[0,0,0,1]})).unwrap();
        document.undo_edit().unwrap();
        document.save_editing_checkpoint();
        let saved=document.editing_state().unwrap();
        document.redo_edit().unwrap();
        let before=document.state.settings.clone();
        let mut requested=before.clone();requested.complexity=7;
        document.request_cancel();
        assert!(!document.finish_cancellable_task(requested.clone()));
        assert_eq!(document.state.settings,before);
        assert_eq!(document.state.status,2);
        assert_eq!(document.state.progress,100);
        assert!(document.snapshot().cancelled);
        assert_eq!(document.restore_committed_editing().unwrap().unwrap(),saved);
        document.redo_edit().unwrap();
        document.begin_task();
        assert!(document.finish_cancellable_task(requested.clone()));
        assert_eq!(document.state.settings,requested);
        assert_eq!(document.state.status,0);
        assert!(!document.editing_changed());
    }
}
