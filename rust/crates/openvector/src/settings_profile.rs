//! Settings profiles keep GUI counters independent of palette length.
use crate::document_controls::{DocumentSettings,read_document_settings,ADVANCED_BATCH_CONTROLS};
use serde_json::{json,Value};
pub const DEFAULT_ADVANCED_PROFILE_NAME:&str="Default Advanced Settings";
#[derive(Clone,Debug,PartialEq)]
pub struct SettingsProfile{pub settings:DocumentSettings,pub color_count:i32,pub basic_color_count:i32,pub palette_color_count:i32,pub recommended_background_mode:i32}
impl SettingsProfile{
    pub fn to_json(&self)->Value{json!({"settings":self.settings.to_json(),"colorCount":self.color_count,"basicColorCount":self.basic_color_count,"paletteColorCount":self.palette_color_count,"recommendedBackgroundMode":self.recommended_background_mode})}
}
pub fn read_settings_profile(input:&Value)->Result<SettingsProfile,String>{
    if !input.is_object(){return Err("Expected a saved settings profile.".into())}
    let settings=read_document_settings(&input["settings"]).map_err(|e|e.to_string())?;
    let count=|name:&str| input[name].as_f64().filter(|x|x.fract()==0.0 && *x>=i32::MIN as f64 && *x<=i32::MAX as f64).map(|x|x as i32).ok_or_else(||format!("Profile {name} must be a signed int32 integer."));
    Ok(SettingsProfile{settings,color_count:count("colorCount")?,basic_color_count:count("basicColorCount")?,palette_color_count:count("paletteColorCount")?,recommended_background_mode:count("recommendedBackgroundMode")?})
}
pub fn default_advanced_profile()->SettingsProfile{SettingsProfile{settings:DocumentSettings{advanced:true,..Default::default()},color_count:2,basic_color_count:0,palette_color_count:2,recommended_background_mode:-1}}
pub fn default_selected_profile()->SettingsProfile{SettingsProfile{settings:DocumentSettings{background:[0.0,0.0,0.0,1.0],..Default::default()},color_count:2,basic_color_count:0,palette_color_count:0,recommended_background_mode:0}}
pub fn default_batch_profile()->SettingsProfile{let mut p=default_selected_profile();p.settings.category=3;p.settings.quality=3;p.settings.palette_mode=3;p.settings.background_mode=1;p.palette_color_count=3;p}
/// Persistence uses QColor's high byte and discards palette entries. Keep the
/// resulting byte/255 doubles in JSON rather than rounding them through f32.
pub fn stored_settings_profile(profile:&SettingsProfile)->Value{
    let mut p=profile.to_json();p["settings"]["palette"]=json!([]);
    p["settings"]["background"]=json!(profile.settings.background.iter().enumerate().map(|(i,v)|if i==3{1.0}else{(((*v as f64*65535.0+0.5).floor() as u32)>>8) as f64/255.0}).collect::<Vec<_>>());p
}
pub fn batch_profile_controls(profile:&SettingsProfile)->Value{
    let s=profile.settings.to_json();let mut result=json!({"colorCount":if profile.settings.advanced{profile.color_count}else{profile.basic_color_count}});
    for name in ["advanced","category","quality","paletteMode","backgroundMode","palette"].iter().chain(ADVANCED_BATCH_CONTROLS.iter()){result[*name]=s[*name].clone()}result
}
pub fn apply_advanced_profile(current:Option<&SettingsProfile>,selected:&SettingsProfile,batch:bool)->Result<SettingsProfile,String>{
    let source=read_settings_profile(&selected.to_json())?;
    if !batch{let mut p=source;p.settings.advanced=true;return Ok(p)}
    let mut target=read_settings_profile(&current.ok_or("Expected a saved settings profile.")?.to_json())?;
    let mut settings=target.settings.to_json();let source_settings=source.settings.to_json();
    for name in ADVANCED_BATCH_CONTROLS{settings.insert(name.into(),source_settings[name].clone());}
    settings.insert("advanced".into(),json!(true));settings.insert("paletteMode".into(),json!(source.settings.palette_mode));settings.insert("palette".into(),source_settings["palette"].clone());
    target.settings=read_document_settings(&Value::Object(settings)).map_err(|e|e.to_string())?;target.color_count=source.color_count;target.palette_color_count=source.palette_color_count;Ok(target)
}

/// Recent Basic summary, ported from native VectorizationSettings 0x440bb0 by
/// static analysis. It is also the recent list's identity, so advanced,
/// flattening, counter and palette-color fields never distinguish entries. An
/// unknown value omits its component; an unknown palette mode therefore keeps
/// the preceding ", ".
pub fn basic_profile_label(profile:&SettingsProfile)->String{
    use std::fmt::Write;let s=&profile.settings;let mut label=String::new();
    for (value,names) in [(s.category,["Photograph","Anti-aliased artwork","Artwork"]),(s.quality,["High","Medium","Low"])]{
        if let Some(name)=usize::try_from(value).ok().and_then(|i|names.get(i)){label.push_str(name);label.push_str(", ")}
    }
    match (s.palette_mode,profile.palette_color_count){
        (0,_)=>label.push_str("Two colors"),(1,count) if count>0=>{let _=write!(label,"{count} colors");},(1,_)=>label.push_str("Fixed colors"),
        (2,_)=>label.push_str("Unlimited colors"),_=>(),
    }
    label
}
/// Recent Basic replay copies the whole stored profile into the selected
/// controls and leaves advanced mode (native vectorizeImageWithMruSettings
/// 0x42b560). Palette runtime state is not part of a profile, so its owner
/// keeps the backup and customized flag.
pub fn apply_basic_profile(stored:&SettingsProfile)->SettingsProfile{let mut profile=stored.clone();profile.settings.advanced=false;profile}

/// Owned outcome of a profile replay. Adopting `profile` as the selected
/// controls remains the caller's explicit step.
#[derive(Clone,Debug)]
pub struct ProfileResult{
    pub profile:SettingsProfile,pub palette_runtime:crate::profile_palette::PaletteRuntime,
    pub state:crate::image_document::DocumentSnapshot,pub palette_requested:bool,pub did_work:std::collections::BTreeMap<String,bool>
}
pub type ProfileStageObserver<'a>=dyn FnMut(&str,&crate::image_document::TaskResult,&SettingsProfile,&crate::profile_palette::PaletteRuntime)->Result<(),String>+'a;
/// Saved Advanced Settings replay. A fixed palette without a count searches 2..12.
pub fn run_advanced_profile(document:&mut crate::image_document::ImageDocument,input:&SettingsProfile,runtime:&crate::profile_palette::PaletteRuntime,on_progress:Option<&mut crate::image_document::DocumentObserver>,on_stage:Option<&mut ProfileStageObserver>)->Result<Option<ProfileResult>,String>{
    let profile=apply_advanced_profile(None,input,false)?;
    if profile.settings.palette_mode==3{return Ok(None)}
    if !(2..=24).contains(&profile.color_count){return Err("Number of colors must be an integer from 2 through 24.".into())}
    let count=profile.palette_color_count;
    let bounds=match profile.settings.palette_mode{0=>Some((2,2)),1 if count!=0=>Some((count,count)),1=>Some((2,12)),_=>None};
    run_profile(document,profile,bounds,runtime,on_progress,on_stage).map(Some)
}
/// Recent Basic replay (native vectorizeImageWithMruSettings 0x42b560, static
/// analysis). Two colors searches exactly 2 and a fixed palette exactly its
/// stored count. There is no zero fallback: the palette task rejects counts
/// outside 2..=24. Unlimited colors skips the search, and any other mode
/// processes nothing.
pub fn run_basic_profile(document:&mut crate::image_document::ImageDocument,input:&SettingsProfile,runtime:&crate::profile_palette::PaletteRuntime,on_progress:Option<&mut crate::image_document::DocumentObserver>,on_stage:Option<&mut ProfileStageObserver>)->Result<Option<ProfileResult>,String>{
    let profile=apply_basic_profile(input);let count=profile.palette_color_count;
    let bounds=match profile.settings.palette_mode{0=>Some((2,2)),1=>Some((count,count)),2=>None,_=>return Ok(None)};
    run_profile(document,profile,bounds,runtime,on_progress,on_stage).map(Some)
}
/// Shared replay chain: an optional palette search over inclusive `bounds`
/// whose lowest-score candidate is chosen through the palette command, then
/// segmentation, smoothing and fitting. A cancelled stage is not published.
fn run_profile(document:&mut crate::image_document::ImageDocument,mut profile:SettingsProfile,bounds:Option<(i32,i32)>,runtime:&crate::profile_palette::PaletteRuntime,mut on_progress:Option<&mut crate::image_document::DocumentObserver>,mut on_stage:Option<&mut ProfileStageObserver>)->Result<ProfileResult,String>{
    use crate::profile_palette::{apply_palette_command,PaletteProfile,PaletteCommand};
    let mut runtime=runtime.clone();let mut did_work=std::collections::BTreeMap::new();
    if let Some((lower,upper))=bounds{
        let mut controls=Value::Object(profile.settings.to_json());controls["lower"]=json!(lower);controls["upper"]=json!(upper);
        let task=document.run_task("palette",&controls,on_progress.as_mut().map(|o|&mut **o as &mut crate::image_document::DocumentObserver))?;did_work.insert("palette".into(),task.did_work);
        let mut chosen=lower as usize;for count in lower as usize+1..=upper as usize{if task.state.scores[count]<task.state.scores[chosen]{chosen=count}}
        let palette=task.state.palettes[chosen].chunks_exact(4).flat_map(|c|c.iter().rev().map(|v|*v as f64)).collect();
        let mut color_count=profile.color_count as f64;let mut palette_count=profile.palette_color_count as f64;
        apply_palette_command(&mut PaletteProfile{color_count:&mut color_count,palette_color_count:&mut palette_count,palette:&mut profile.settings.palette},&mut runtime,&PaletteCommand::Choose(palette))?;
        profile.color_count=color_count as i32;profile.palette_color_count=palette_count as i32;
        if let Some(cb)=on_stage.as_deref_mut(){cb("palette",&task,&profile,&runtime)?}
    }
    let mut run_stage=|stage:&str|->Result<crate::image_document::TaskResult,String>{
        let task=document.run_task(stage,&Value::Object(profile.settings.to_json()),on_progress.as_mut().map(|o|&mut **o as &mut crate::image_document::DocumentObserver))?;
        if task.did_work&&task.state.status==2{return Err(crate::image_document::CANCEL_TASK.into())}
        did_work.insert(stage.into(),task.did_work);if let Some(cb)=on_stage.as_deref_mut(){cb(stage,&task,&profile,&runtime)?}
        Ok(task)
    };
    run_stage("segmentation")?;run_stage("smoothing")?;let fitted=run_stage("fitting")?;
    Ok(ProfileResult{profile,palette_runtime:runtime,state:document.refresh_snapshot(fitted.state),palette_requested:bounds.is_some(),did_work})
}
