//! Shared pointer and QColor wire validation for both editors.
use serde_json::Value;
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct EditorPointer {pub previous_point:[f64;2]}
impl Default for EditorPointer {fn default()->Self{Self{previous_point:[-1.0,-1.0]}}}
#[derive(Debug,Clone,PartialEq)]
pub enum EditorGesture {Select(String),Color([f32;4]),Pointer{phase:PointerPhase,point:[f64;2],buttons:u32,modifiers:u32}}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum PointerPhase {Press,Move,Release}
pub fn read_editor_point(point:&Value)->Result<[f64;2],String>{let array=point.as_array().filter(|a|a.len()==2).ok_or("Gesture point must contain two finite world coordinates.")?;let x=array[0].as_f64().unwrap_or(f64::NAN);let y=array[1].as_f64().unwrap_or(f64::NAN);if !x.is_finite()||!y.is_finite(){return Err("Gesture point must contain two finite world coordinates.".into());}Ok([x,y])}
pub(crate) fn read_color(value:&Value)->Result<Vec<f32>,String>{let array=value.as_array().ok_or("Palette must contain four normalized BGRA channels per color.")?;let values:Vec<f64>=array.iter().map(|v|v.as_f64().unwrap_or(f64::NAN)).collect();crate::pixel_input::read_palette_colors(&values).map_err(|e|e.0)}
pub fn read_editor_gesture(input:&Value,tools:&[&str])->Result<EditorGesture,String>{
    if !input.is_object(){return Err("Expected an editor gesture.".into());}
    match input["phase"].as_str(){
        Some("select")=>{let tool=input["tool"].as_str().unwrap_or("");if !tools.contains(&tool){return Err(format!("Gesture tool must be {}.",tools.join(", ")));}Ok(EditorGesture::Select(tool.into()))},
        Some("color")=>{let color=read_color(&input["color"])?;if color.len()!=4{return Err("Gesture color must contain four BGRA channels.".into());}let color=crate::pixel_input::quantize_qcolor_channels(&color);Ok(EditorGesture::Color(color.try_into().unwrap()))},
        Some(phase @ ("press"|"move"|"release"))=>{let point=read_editor_point(&input["point"])?;let mut masks=[0u32;3];for (i,name) in ["buttons","button","modifiers"].iter().enumerate(){let number=input.get(*name).and_then(Value::as_f64).unwrap_or(if i>0&&input.get(*name).is_none(){0.0}else{f64::NAN});if !number.is_finite()||number.fract()!=0.0||number<0.0||number>u32::MAX as f64{return Err(format!("Gesture {name} must be an unsigned int32 bit mask."));}masks[i]=number as u32;}Ok(EditorGesture::Pointer{phase:match phase{"press"=>PointerPhase::Press,"move"=>PointerPhase::Move,_=>PointerPhase::Release},point,buttons:masks[0],modifiers:masks[2]})},
        _=>Err("Gesture phase must be select, color, press, move or release.".into())
    }
}
