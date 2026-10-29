//! Logical background visibility and native replay histories.
use serde_json::Value;
use crate::contours::Geometry;
use crate::editor_input::{EditorGesture,EditorPointer,PointerPhase,read_editor_gesture};
#[derive(Debug,Clone,PartialEq,Eq,serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundSnapshot {pub count:usize,pub active:bool,pub tool:String,pub hidden_regions:Vec<usize>,pub undo:Vec<usize>,pub redo:Vec<usize>}
#[derive(Debug,Clone)]
pub struct BackgroundEditor {hidden:Vec<bool>,undo:Vec<usize>,redo:Vec<usize>,active:bool,tool:String}
impl Default for BackgroundEditor {fn default()->Self{Self{hidden:Vec::new(),undo:Vec::new(),redo:Vec::new(),active:false,tool:"pan".into()}}}
impl BackgroundEditor {
    pub fn new()->Self{Self::default()}
    pub fn snapshot(&self)->BackgroundSnapshot{BackgroundSnapshot{count:self.hidden.len(),active:self.active,tool:self.tool.clone(),hidden_regions:self.hidden.iter().enumerate().filter(|(_,hidden)|**hidden).map(|(i,_)|i).collect(),undo:self.undo.clone(),redo:self.redo.clone()}}
    pub fn begin(&mut self)->Result<BackgroundSnapshot,String>{if self.hidden.is_empty(){return Err("Background editing needs a completed vector result.".into());}self.active=true;self.tool="background".into();Ok(self.snapshot())}
    pub fn finish(&mut self)->BackgroundSnapshot{self.active=false;self.tool="pan".into();self.snapshot()}
    fn clear(&mut self,count:usize){self.hidden=vec![false;count];self.redo=std::mem::take(&mut self.undo);}
    pub fn accept_vectors(&mut self,count:usize){self.clear(count);}
    fn toggle_region(&mut self,region:usize){if let Some(hidden)=self.hidden.get_mut(region){*hidden=!*hidden;}}
    pub fn toggle(&mut self,point:[f64;2],width:i32,height:i32,indices:&[i32],geometry:Option<&Geometry>)->Result<BackgroundSnapshot,String>{
        if point.iter().any(|v|!v.is_finite()){return Err("Gesture point must contain two finite world coordinates.".into());}
        let geometry=geometry.ok_or("Background hit testing needs completed contour boundaries. Update the vector first.")?;
        let region=crate::vector_output::find_contour_region(width,height,indices,geometry,point);
        if region!=-1{self.toggle_region(region as usize);self.undo.push(region as usize);self.redo.clear();}Ok(self.snapshot())
    }
    pub fn gesture(&mut self,input:&Value,width:i32,height:i32,indices:&[i32],geometry:Option<&Geometry>,pointer:&mut EditorPointer)->Result<BackgroundSnapshot,String>{
        match read_editor_gesture(input,&["pan","background"])?{
            EditorGesture::Select(tool)=>self.tool=if tool==self.tool{"pan".into()}else{tool},
            EditorGesture::Color(_)=>return Err("Background editing does not change the paint color.".into()),
            EditorGesture::Pointer{phase:PointerPhase::Press,point,buttons,..} if self.tool=="background"&&buttons&1!=0=>{pointer.previous_point=point;return self.toggle(point,width,height,indices,geometry);},_=>{}
        }Ok(self.snapshot())
    }
    pub fn undo(&mut self)->BackgroundSnapshot{if let Some(region)=self.undo.pop(){self.toggle_region(region);self.redo.insert(0,region);}self.snapshot()}
    pub fn redo(&mut self)->BackgroundSnapshot{if !self.redo.is_empty(){let region=self.redo.remove(0);self.toggle_region(region);self.undo.push(region);}self.snapshot()}
    pub fn revert(&mut self)->BackgroundSnapshot{self.clear(self.hidden.len());self.snapshot()}
}
/// Document dispatch stays with the document state owner. This parser preserves
/// ordered nonempty gesture batches without applying a partially validated plan.
#[derive(Debug,Clone,PartialEq)]
pub enum BackgroundCommand {Begin,Toggle([f64;2]),Undo,Redo,Revert,Finish,Gesture(Vec<Value>)}
pub fn read_background_command(input:&Value)->Result<BackgroundCommand,String>{
    if !input.is_object(){return Err("Expected a background editing command.".into());}
    Ok(match input["action"].as_str(){Some("begin")=>BackgroundCommand::Begin,Some("toggle")=>BackgroundCommand::Toggle(crate::editor_input::read_editor_point(&input["point"])?),Some("undo")=>BackgroundCommand::Undo,Some("redo")=>BackgroundCommand::Redo,Some("revert")=>BackgroundCommand::Revert,Some("finish")=>BackgroundCommand::Finish,Some("gesture")=>BackgroundCommand::Gesture(input["events"].as_array().filter(|a|!a.is_empty()).ok_or("Submit a nonempty array of background gesture events.")?.clone()),_=>return Err("Choose begin, gesture, toggle, undo, redo, revert or finish for background editing.".into())})
}
