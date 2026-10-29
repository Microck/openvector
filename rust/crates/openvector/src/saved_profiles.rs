//! Ordered native saved-profile and recent-basic lists with atomic preference-file persistence.
use crate::settings_profile::{basic_profile_label,default_advanced_profile,read_settings_profile,stored_settings_profile,SettingsProfile,DEFAULT_ADVANCED_PROFILE_NAME};
use serde_json::{json,Map,Value};
use std::{fs::{self,OpenOptions},io::{BufWriter,Write},path::Path,sync::atomic::{AtomicU64,Ordering}};
#[derive(Clone,Debug,PartialEq)]
pub struct SavedProfile{pub name:String,pub profile:SettingsProfile}
#[derive(Clone,Debug,PartialEq)]
pub struct ProfileRename{pub index:isize,pub changed:bool,pub replaced_index:isize}
#[derive(Clone,Debug)]
pub struct SavedAdvancedProfiles{entries:Vec<SavedProfile>}
impl Default for SavedAdvancedProfiles{fn default()->Self{Self{entries:vec![SavedProfile{name:DEFAULT_ADVANCED_PROFILE_NAME.into(),profile:default_advanced_profile()}]}}}
impl SavedAdvancedProfiles{
    pub fn new(entries:&Value)->Result<Self,String>{
        let entries=entries.as_array().ok_or("Saved profiles must be an array.")?;let mut list=Self{entries:Vec::new()};
        for entry in entries.iter().rev(){let name=entry["name"].as_str().ok_or("Profile name must be a string.")?;list.put_json(name,&entry["profile"])?;}
        list.put(DEFAULT_ADVANCED_PROFILE_NAME,default_advanced_profile());Ok(list)
    }
    pub fn len(&self)->usize{self.entries.len()}
    pub fn is_empty(&self)->bool{self.entries.is_empty()}
    pub fn snapshot(&self)->Vec<SavedProfile>{self.entries.clone()}
    pub fn reset(&mut self){*self=Self::default()}
    pub fn put_json(&mut self,name:&str,profile:&Value)->Result<(),String>{let p=if name==DEFAULT_ADVANCED_PROFILE_NAME{default_advanced_profile()}else{read_settings_profile(profile)?};self.put(name,p);Ok(())}
    pub fn put(&mut self,name:&str,profile:SettingsProfile){
        self.remove(name);self.entries.insert(0,SavedProfile{name:name.into(),profile:if name==DEFAULT_ADVANCED_PROFILE_NAME{default_advanced_profile()}else{profile}});self.entries.truncate(200)
    }
    pub fn select(&mut self,name:&str)->(isize,Option<SettingsProfile>){match self.entries.iter().position(|e|e.name==name){None=>(-1,None),Some(i)=>{let e=self.entries.remove(i);let p=e.profile.clone();self.entries.insert(0,e);(i as isize,Some(p))}}}
    pub fn remove(&mut self,name:&str)->isize{match self.entries.iter().position(|e|e.name==name){None=>-1,Some(i)=>{self.entries.remove(i);i as isize}}}
    pub fn rename(&mut self,name:&str,new_name:&str,force:bool)->Result<ProfileRename,String>{
        let index=self.entries.iter().position(|e|e.name==name);let mut result=ProfileRename{index:index.map_or(-1,|i|i as isize),changed:false,replaced_index:-1};
        let Some(index)=index else{return Ok(result)};if new_name.is_empty()||new_name==name{return Ok(result)}
        let replaced=self.entries.iter().position(|e|e.name==new_name);if replaced.is_some()&&!force{return Err("A profile with this exact name already exists. Confirm replacement.".into())}
        self.entries[index].name=new_name.into();if let Some(i)=replaced{self.entries.remove(i);result.replaced_index=i as isize}result.changed=true;Ok(result)
    }
    pub fn read_file(path:impl AsRef<Path>)->Result<Self,String>{
        match read_array_file(path.as_ref(),"profiles","Profile file must contain one profiles array.")?{None=>Ok(Self::default()),Some(profiles)=>Self::new(&profiles)}
    }
    pub fn write_file(&self,path:impl AsRef<Path>)->Result<(),String>{
        write_array_file(path.as_ref(),"profiles",self.entries.iter().map(|e|json!({"name":e.name,"profile":stored_settings_profile(&e.profile)})).collect())
    }
}

/// One Recent Basic Settings entry; the derived label is its identity.
#[derive(Clone,Debug,PartialEq)]
pub struct RecentBasicProfile{pub label:String,pub profile:SettingsProfile}
/// Native Recent Basic Settings (MainModel+0x18c labels beside +0x19c
/// settings), ported from static analysis of addMruSettings 0x42d890: newest
/// first, five stored and four shown.
#[derive(Clone,Debug,Default)]
pub struct RecentBasicProfiles{entries:Vec<RecentBasicProfile>}
const RECENT_BASIC_CAPACITY:usize=5;
const RECENT_BASIC_VISIBLE:usize=4;
impl RecentBasicProfiles{
    pub fn entries(&self)->&[RecentBasicProfile]{&self.entries}
    /// The native list widget hides rows from index four, so the fifth entry
    /// persists without being offered.
    pub fn visible_entries(&self)->&[RecentBasicProfile]{&self.entries[..self.entries.len().min(RECENT_BASIC_VISIBLE)]}
    /// Record the current profile at a fitting completion. Only a successful
    /// (status 0) single-image fitting that did work is eligible, whatever
    /// workflow ran it. The result reports eligibility, so it is true even when
    /// the label was already first and nothing moved.
    pub fn record_completion(&mut self,profile:&SettingsProfile,status:u32,did_work:bool,batch:bool)->bool{
        if status!=0||!did_work||batch{return false}
        self.record(basic_profile_label(profile),||profile.clone());true
    }
    pub fn clear(&mut self){self.entries.clear()}
    /// Preference projection, newest first. Labels are derived on load, never stored.
    pub fn stored_profiles(&self)->Value{Value::Array(self.entries.iter().map(|e|stored_settings_profile(&e.profile)).collect())}
    /// Native readSettings replays stored entries from oldest to newest through
    /// the recording rule: an older duplicate keeps its profile and the lowest
    /// five indices survive.
    pub fn from_stored_profiles(stored:&Value)->Result<Self,String>{
        let stored=stored.as_array().ok_or("Recent basic settings must be an array.")?;let mut list=Self::default();
        for entry in stored.iter().rev(){let profile=read_settings_profile(entry)?;list.record(basic_profile_label(&profile),||profile)}
        Ok(list)
    }
    pub fn read_file(path:impl AsRef<Path>)->Result<Self,String>{
        match read_array_file(path.as_ref(),"recentBasicSettings","Recent settings file must contain one recentBasicSettings array.")?{None=>Ok(Self::default()),Some(stored)=>Self::from_stored_profiles(&stored)}
    }
    pub fn write_file(&self,path:impl AsRef<Path>)->Result<(),String>{write_array_file(path.as_ref(),"recentBasicSettings",self.stored_profiles())}
    /// addMruSettings: a known label moves its stored profile to the front and
    /// drops the incoming one; a new label is prepended and the list trimmed.
    fn record(&mut self,label:String,profile:impl FnOnce()->SettingsProfile){
        match self.entries.iter().position(|e|e.label==label){
            Some(index)=>self.entries[..=index].rotate_right(1),
            None=>{self.entries.insert(0,RecentBasicProfile{label,profile:profile()});self.entries.truncate(RECENT_BASIC_CAPACITY)}
        }
    }
}

/// Read a `{key:[...]}` preference file. A missing file is `None`; any other
/// content must be exactly that one array member.
fn read_array_file(path:&Path,key:&str,shape:&str)->Result<Option<Value>,String>{
    let encoded=match fs::read_to_string(path){Ok(s)=>s,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e.to_string())};
    let mut value:Value=serde_json::from_str(&encoded).map_err(|e|e.to_string())?;
    if value.as_object().is_none_or(|o|o.len()!=1)||!value[key].is_array(){return Err(shape.into())}Ok(Some(value[key].take()))
}
/// Write `{key:array}` to a fresh sibling file and rename it over `path`, so a
/// failed write keeps the previous file and leaves no temporary behind.
fn write_array_file(path:&Path,key:&str,array:Value)->Result<(),String>{
    static COUNTER:AtomicU64=AtomicU64::new(0);let dir=path.parent().unwrap_or(Path::new("."));
    let (temporary,file)=loop{let temporary=dir.join(format!(".openvector-profiles-{}-{}.tmp",std::process::id(),COUNTER.fetch_add(1,Ordering::Relaxed)));match OpenOptions::new().write(true).create_new(true).open(&temporary){Ok(f)=>break(temporary,f),Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,Err(e)=>return Err(e.to_string())}};
    let mut object=Map::new();object.insert(key.into(),array);
    let result=(||{let mut file=BufWriter::new(file);serde_json::to_writer(&mut file,&Value::Object(object)).map_err(|e|e.to_string())?;file.write_all(b"\n").map_err(|e|e.to_string())?;file.flush().map_err(|e|e.to_string())?;drop(file);fs::rename(&temporary,path).map_err(|e|e.to_string())})();
    let cleanup=fs::remove_file(&temporary);if result.is_ok(){if let Err(e)=cleanup{if e.kind()!=std::io::ErrorKind::NotFound{return Err(e.to_string())}}}result
}
