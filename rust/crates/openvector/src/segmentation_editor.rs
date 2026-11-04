//! Displayed-color bitmap editing with native x87 sampling and replay records.
use std::collections::BTreeSet;
use num_bigint::BigInt;
use num_traits::{One,ToPrimitive};
use serde_json::Value;
use crate::extended_arithmetic::{fixed_double,round_significand,binary_double,unit,PRECISION,coordinate_int32};
use crate::editor_input::{EditorGesture,EditorPointer,PointerPhase,read_editor_gesture,read_color};
use crate::decimal_format::stream_general_decimal;
const TOOLS:[&str;5]=["pan","pencil","fill","zap","picker"];
fn extended(v:BigInt)->BigInt{round_significand(&v,64)}
fn multiply(a:&BigInt,b:&BigInt)->BigInt{extended(a*b/unit())}
fn store_double(v:&BigInt)->f64{binary_double(v,-(PRECISION as i64))}
fn store_float(v:&BigInt)->f64{store_double(&round_significand(v,24))}
fn stroke_samples(from:[f64;2],to:[f64;2],mut paint:impl FnMut(i64,i64)->bool)->bool{
    let [start_x,start_y]=to;let [end_x,end_y]=from;
    if start_x.trunc()==end_x.trunc()&&start_y.trunc()==end_y.trunc(){return paint(start_x.trunc() as i64,start_y.trunc() as i64);}
    let x_origin=fixed_double(start_x);let y_origin=fixed_double(start_y);
    let dx=extended(fixed_double(end_x)-&x_origin);let dy=extended(fixed_double(end_y)-&y_origin);
    let square=extended(multiply(&dx,&dx)+multiply(&dy,&dy));let radicand=square*unit();
    let mut root=BigInt::one()<<((radicand.magnitude().bits()+1)/2) as usize;
    loop{let next=(&root+&radicand/&root)/2;if next>=root{break;}root=next;}
    // A subpixel segment can cross a pixel boundary with fewer than two
    // samples. Include both endpoints instead of painting an unrelated origin.
    let samples=(extended(extended(root)*4)/unit()).to_usize().expect("bounded image stroke").max(2);
    let mut changed=false;let (mut previous_x,mut previous_y)=(-1,-1);
    for sample in 0..samples{let ratio=extended(BigInt::from(sample)*unit()/BigInt::from(samples-1));let x_product=fixed_double(store_double(&multiply(&dx,&ratio)));let x=(extended(x_product+&x_origin)/unit()).to_i64().unwrap();let y=(extended(multiply(&dy,&ratio)+&y_origin)/unit()).to_i64().unwrap();if x!=previous_x||y!=previous_y{changed=paint(x,y)||changed;}previous_x=x;previous_y=y;}changed
}
#[derive(Debug,Clone,PartialEq,serde::Serialize)]
pub struct ToolRecord{pub kind:i32,pub x:i32,pub y:i32,pub coordinates:[f64;4],pub argb:[f64;4],pub key:String,#[serde(skip)] pub joins_previous:bool}
impl ToolRecord{fn new(kind:i32,x:i32,y:i32,coordinates:[f64;4],argb:[f64;4])->Self{let key=format!("{kind} {x} {y} {} [{}]",coordinates.map(stream_general_decimal).join(" "),argb.map(stream_general_decimal).join(", "));Self{kind,x,y,coordinates,argb,key,joins_previous:false}}}
#[derive(Debug,Clone,PartialEq,serde::Serialize)]
pub struct EditorCheckpoint{pub undo:Vec<ToolRecord>,pub redo:Vec<ToolRecord>}
#[derive(Debug,Clone,PartialEq,serde::Serialize)]
pub struct EditorSnapshot{pub width:usize,pub height:usize,pub bgra:Vec<u8>,pub undo:Vec<ToolRecord>,pub redo:Vec<ToolRecord>,pub key:String,pub tool:String,pub color:[f32;4]}
#[derive(Debug,Clone)]
pub struct SegmentationEditor{width:usize,height:usize,base:Vec<u8>,pixels:Vec<u8>,source:Vec<u8>,background:[f64;4],background_mode:i32,undo:Vec<ToolRecord>,redo:Vec<ToolRecord>,tool:String,previous_tool:String,color:[f32;4],grouped_stroke_active:bool,grouped_stroke_has_record:bool}
fn read_command(input:&Value)->Result<(ToolRecord,bool),String>{
    let tool=input["tool"].as_str().unwrap_or("");if !["fill","pencil","zap"].contains(&tool){return Err("Edit tool must be fill, pencil or zap.".into());}
    let commit=match input.get("commit"){None=>true,Some(v)=>v.as_bool().ok_or("Edit commit must be boolean.")?};
    let default=serde_json::json!([0,0,0,1]);let color=input.get("color").filter(|v|!v.is_null()).unwrap_or(if tool=="zap"{&default}else{&Value::Null});let color=read_color(color)?;
    if color.len()!=4{return Err("Edit color must contain four normalized BGRA channels.".into());}
    let mut x=0;let mut y=0;let mut coordinates=[0.0;4];
    if tool=="pencil"{for (i,name) in ["from","to"].iter().enumerate(){let endpoint=crate::editor_input::read_editor_point(&input[*name]).map_err(|_|"Pencil from and to must each contain two finite coordinates.".to_string())?;coordinates[i*2..i*2+2].copy_from_slice(&endpoint);}}
    else{let mut values=[0;2];for (i,name) in ["x","y"].iter().enumerate(){let value=input[*name].as_f64().unwrap_or(f64::NAN);if !value.is_finite()||value.fract()!=0.0||value<i32::MIN as f64||value>i32::MAX as f64{return Err("Edit x and y must be signed int32 coordinates.".into());}values[i]=value as i32;}x=values[0];y=values[1];}
    Ok((ToolRecord::new(if tool=="pencil"{3}else if tool=="fill"{5}else{6},x,y,coordinates,std::array::from_fn(|c|color[3-c] as f64)),commit))
}
fn packed(bytes:&[u8],pixel:usize)->u32{u32::from_le_bytes(bytes[pixel*4..pixel*4+4].try_into().unwrap())}
fn zap_distance(first:u32,second:u32)->BigInt{
    let mut sum=BigInt::from(0);for shift in [16,8,0]{let difference=fixed_double((((second>>shift)&255) as f64-((first>>shift)&255) as f64)*(1.0/255.0));sum=extended(sum+multiply(&difference,&difference));}
    let difference=multiply(&fixed_double((second>>24) as f64-(first>>24) as f64),&fixed_double(1.0/255.0));extended(sum+multiply(&difference,&difference))
}
impl SegmentationEditor{
    pub fn new(width:i64,height:i64,bgra:&[u8],source:&[u8],background:&[f64],color:&[f64],background_mode:i32)->Result<Self,String>{
        let raster=crate::region_data::read_raster_input(width,height,bgra).map_err(|e|e.0)?;crate::region_data::read_raster_input(width,height,source).map_err(|e|e.0)?;
        let background=crate::pixel_input::read_palette_colors(background).map_err(|e|e.0)?;if background.len()!=4{return Err("Editor background must contain four BGRA channels.".into());}
        let color=crate::pixel_input::read_palette_colors(color).map_err(|e|e.0)?;if color.len()!=4{return Err("Gesture color must contain four BGRA channels.".into());}
        let color=crate::pixel_input::quantize_qcolor_channels(&color).try_into().unwrap();
        Ok(Self{width:raster.width,height:raster.height,base:bgra.to_vec(),pixels:bgra.to_vec(),source:source.to_vec(),background:std::array::from_fn(|c|(background[c] as f64*65535.0).round()/65535.0),background_mode,undo:Vec::new(),redo:Vec::new(),tool:"pan".into(),previous_tool:"pan".into(),color,grouped_stroke_active:false,grouped_stroke_has_record:false})
    }
    pub fn history_state(&self)->(usize,usize){(self.undo.len(),self.redo.len())}
    pub fn history_checkpoint(&self)->EditorCheckpoint{EditorCheckpoint{undo:self.undo.clone(),redo:self.redo.clone()}}
    /// Native Back compares active serialized records, not redo or raw float bits.
    pub fn matches_active_checkpoint(&self,checkpoint:Option<&EditorCheckpoint>)->bool{
        let active=checkpoint.map_or(&[][..],|checkpoint|checkpoint.undo.as_slice());
        self.undo.len()==active.len()&&self.undo.iter().zip(active).all(|(current,saved)|current.key==saved.key)
    }
    pub fn restore_checkpoint(&mut self,checkpoint:&EditorCheckpoint)->Result<EditorSnapshot,String>{self.grouped_stroke_active=false;self.grouped_stroke_has_record=false;self.undo=checkpoint.undo.clone();self.redo=checkpoint.redo.clone();self.replay()?;Ok(self.snapshot())}
    pub fn snapshot(&self)->EditorSnapshot{EditorSnapshot{width:self.width,height:self.height,bgra:self.pixels.clone(),undo:self.undo.clone(),redo:self.redo.clone(),key:self.undo.iter().map(|r|r.key.as_str()).collect(),tool:self.tool.clone(),color:self.color}}
    pub fn apply(&mut self,input:&Value)->Result<EditorSnapshot,String>{self.grouped_stroke_active=false;self.grouped_stroke_has_record=false;let (record,commit)=read_command(input)?;self.apply_record(record,commit)?;Ok(self.snapshot())}
    fn apply_record(&mut self,record:ToolRecord,commit:bool)->Result<(),String>{if self.execute(&record)?&&commit{self.undo.push(record);self.redo.clear();}Ok(())}
    pub fn gesture(&mut self,input:&Value,pointer:&mut EditorPointer)->Result<EditorSnapshot,String>{
        match read_editor_gesture(input,&TOOLS)?{
            EditorGesture::Select(tool)=>{let next=if tool==self.tool{"pan".into()}else{tool};self.previous_tool=std::mem::replace(&mut self.tool,next);},
            EditorGesture::Color(color)=>self.color=color,
            EditorGesture::Pointer{phase,point,buttons,..}=>{
                if self.tool=="picker"{if phase==PointerPhase::Release&&self.in_bounds(point){let pixel=point[1].trunc() as usize*self.width+point[0].trunc() as usize;let color=packed(&self.pixels,pixel);self.color=std::array::from_fn(|c|(((color>>(c*8))&255) as f64/255.0) as f32);self.tool=self.previous_tool.clone();}}
                else if buttons&1!=0{let x=coordinate_int32(point[0]);let y=coordinate_int32(point[1]);let argb=std::array::from_fn(|c|self.color[3-c] as f64);
                    if self.tool=="pencil"&&(phase==PointerPhase::Press||phase==PointerPhase::Move){if phase==PointerPhase::Press{pointer.previous_point=point;}self.apply_record(ToolRecord::new(3,x,y,[point[0],point[1],pointer.previous_point[0],pointer.previous_point[1]],argb),true)?;pointer.previous_point=point;}
                    else if phase==PointerPhase::Press&&(self.tool=="fill"||self.tool=="zap"){self.apply_record(ToolRecord::new(if self.tool=="fill"{5}else{6},x,y,[-1.0;4],argb),true)?;}
                }
            }
        }Ok(self.snapshot())
    }
    /// GUI-only grouping preserves the engine's reference-compatible per-event API.
    pub fn grouped_gesture(&mut self,input:&Value,pointer:&mut EditorPointer)->Result<EditorSnapshot,String>{
        let phase=input["phase"].as_str().unwrap_or("");
        let left=input["buttons"].as_u64().unwrap_or(0)&1!=0;
        let painting=self.tool=="pencil"&&left&&(phase=="press"||phase=="move");
        if phase=="press" || !painting { self.grouped_stroke_active=false; self.grouped_stroke_has_record=false; }
        if painting && !self.grouped_stroke_active { self.grouped_stroke_active=true; self.grouped_stroke_has_record=false; }
        let before=self.undo.len();
        self.gesture(input,pointer)?;
        if painting && self.undo.len()>before {
            self.undo.last_mut().unwrap().joins_previous=self.grouped_stroke_has_record;
            self.grouped_stroke_has_record=true;
        }
        Ok(self.snapshot())
    }
    pub fn grouped_undo(&mut self)->Result<EditorSnapshot,String>{
        self.grouped_stroke_active=false;self.grouped_stroke_has_record=false;
        if !self.undo.is_empty(){
            let mut start=self.undo.len()-1;
            while start>0 && self.undo[start].joins_previous {start-=1;}
            let records=self.undo.split_off(start);
            self.redo.splice(0..0,records);
            self.replay()?;
        }
        Ok(self.snapshot())
    }
    pub fn grouped_redo(&mut self)->Result<EditorSnapshot,String>{
        self.grouped_stroke_active=false;self.grouped_stroke_has_record=false;
        if !self.redo.is_empty(){
            let mut count=1;
            while count<self.redo.len() && self.redo[count].joins_previous {count+=1;}
            for _ in 0..count {
                let record=self.redo.remove(0);
                self.execute(&record)?;
                self.undo.push(record);
            }
        }
        Ok(self.snapshot())
    }
    pub fn end_editing(&mut self){self.tool="pan".into();self.grouped_stroke_active=false;self.grouped_stroke_has_record=false;}
    pub fn undo(&mut self)->Result<EditorSnapshot,String>{if let Some(record)=self.undo.pop(){self.redo.insert(0,record);self.replay()?;}Ok(self.snapshot())}
    pub fn redo(&mut self)->Result<EditorSnapshot,String>{if !self.redo.is_empty(){let record=self.redo.remove(0);self.execute(&record)?;self.undo.push(record);}Ok(self.snapshot())}
    pub fn revert(&mut self)->Result<EditorSnapshot,String>{self.grouped_stroke_active=false;self.grouped_stroke_has_record=false;self.pixels.copy_from_slice(&self.base);let mut records=std::mem::take(&mut self.undo);records.append(&mut self.redo);self.redo=records;Ok(self.snapshot())}
    fn replay(&mut self)->Result<(),String>{self.pixels.copy_from_slice(&self.base);for i in 0..self.undo.len(){let record=self.undo[i].clone();self.execute(&record)?;}Ok(())}
    fn in_bounds(&self,[x,y]:[f64;2])->bool{x>=0.0&&y>=0.0&&x<self.width as f64&&y<self.height as f64}
    fn write(&mut self,pixel:usize,color:u32){self.pixels[pixel*4..pixel*4+4].copy_from_slice(&color.to_le_bytes());}
    fn component(&self,seed:usize)->(Vec<usize>,Vec<u32>){let source=packed(&self.pixels,seed);let mut queue=Vec::with_capacity(self.width*self.height);let mut seen=vec![false;self.width*self.height];let mut neighbors=BTreeSet::new();queue.push(seed);seen[seed]=true;let mut head=0;
        while head<queue.len(){let pixel=queue[head];head+=1;let x=pixel%self.width;let mut visit=|p:usize|{let color=packed(&self.pixels,p);if color!=source{neighbors.insert(color);}else if !seen[p]{seen[p]=true;queue.push(p);}};if x>0{visit(pixel-1);}if x+1<self.width{visit(pixel+1);}if pixel>=self.width{visit(pixel-self.width);}if pixel+self.width<self.width*self.height{visit(pixel+self.width);}}
        (queue,neighbors.into_iter().collect())
    }
    fn source_color(&self,pixel:usize)->u32{
        if self.background_mode!=1&&self.background_mode!=2{return packed(&self.source,pixel);}
        let alpha=self.source[pixel*4+3] as f64/255.0;let remaining=1.0-alpha;let background_alpha=self.background[3];let mut bytes=[0u8;4];bytes[3]=((remaining*background_alpha+alpha)*255.0).trunc() as u8;
        for c in 0..3{bytes[c]=(((self.source[pixel*4+c] as f64/255.0)*alpha+remaining*self.background[c]*background_alpha)*255.0).trunc() as u8;}u32::from_le_bytes(bytes)
    }
    fn execute(&mut self,record:&ToolRecord)->Result<bool,String>{
        if record.kind==3{let from=[record.coordinates[0],record.coordinates[1]];let to=[record.coordinates[2],record.coordinates[3]];if !self.in_bounds(from)||!self.in_bounds(to){return Ok(false);}
            let channels:[u32;4]=std::array::from_fn(|c|(record.argb[3-c]*65535.0).round() as u32);let width=self.width;
            return Ok(stroke_samples(from,to,|x,y|{let offset=(y as usize*width+x as usize)*4;let changed=(0..4).any(|c|channels[c]!=self.pixels[offset+c] as u32*257);if changed{for c in 0..4{self.pixels[offset+c]=(channels[c]>>8) as u8;}}changed}));
        }
        if !self.in_bounds([record.x as f64,record.y as f64]){return Ok(false);}let seed=record.y as usize*self.width+record.x as usize;
        if record.kind==5{let bytes:[u8;4]=std::array::from_fn(|c|((record.argb[3-c]*65535.0).round() as u32>>8) as u8);let target=u32::from_le_bytes(bytes);if target==packed(&self.pixels,seed){return Ok(false);}for pixel in self.component(seed).0{self.write(pixel,target);}return Ok(true);}
        let (pixels,neighbors)=self.component(seed);if neighbors.is_empty(){return Err("This region has no neighboring color. Use Fill to change its color.".into());}
        for pixel in pixels{let source=self.source_color(pixel);let mut chosen=neighbors[0];let mut minimum=store_float(&zap_distance(chosen,source));for &neighbor in &neighbors{let distance=zap_distance(neighbor,source);if distance<fixed_double(minimum){chosen=neighbor;minimum=store_float(&distance);}}self.write(pixel,chosen);}Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn short_pencil_segments_only_change_pixels_near_the_stroke() {
        let mut original = vec![0u8; 48 * 48 * 4];
        for pixel in original.chunks_exact_mut(4) { pixel[3] = 255; }
        for (from, to) in [([19.99, 20.1], [20.01, 20.1]), ([19.8, 20.1], [20.1, 20.1])] {
            let mut editor = SegmentationEditor::new(48, 48, &original, &original, &[0.0, 0.0, 0.0, 1.0], &[0.0, 0.0, 1.0, 1.0], 0).unwrap();
            let result = editor.apply(&json!({"tool":"pencil","from":from,"to":to,"color":[0.0,0.0,1.0,1.0]})).unwrap();
            assert_eq!(&result.bgra[..4], &original[..4], "origin changed");
            let changed: Vec<_> = result.bgra.chunks_exact(4).zip(original.chunks_exact(4)).enumerate()
                .filter_map(|(index, (after, before))| (after != before).then_some((index % 48, index / 48))).collect();
            assert_eq!(changed, vec![(19, 20), (20, 20)]);
        }
    }
}
