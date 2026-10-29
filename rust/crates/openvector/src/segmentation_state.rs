//! Document-owned live segmentation state and recovered progress ranges.
use std::sync::LazyLock;
use serde_json::Value;
use crate::region_data::Raster;
use crate::segmentation::{ProgressUpdate,segment_with_moments};

static PROGRESS:LazyLock<Value>=LazyLock::new(||serde_json::from_str(include_str!("../../../../src/segmentation-progress-data.json")).expect("checked-in segmentation progress data"));
#[derive(Debug,Clone,PartialEq)]
pub struct SegmentationSnapshot {
    pub width:usize,pub height:usize,pub indices:Option<Vec<i32>>,pub palette:Vec<f32>,pub counts:Vec<i32>,pub color_indices:Vec<i32>,
}
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct SegmentationProgress {pub phase:i32,pub substep:i32,pub counter:i32,pub fraction:f64,pub percent:i32,pub minimum_pixels:Option<u32>}
#[derive(Debug,Clone)]
pub struct SegmentationState {state:SegmentationSnapshot}
impl SegmentationState {
    pub fn new(previous:Option<&SegmentationSnapshot>,width:usize,height:usize,palette:Option<&[f32]>)->Self {
        let same=previous.is_some_and(|p|p.width==width&&p.height==height);
        Self{state:SegmentationSnapshot{width,height,indices:if same{previous.and_then(|p|p.indices.clone())}else{None},palette:palette.map(<[f32]>::to_vec).unwrap_or_else(||previous.map_or_else(Vec::new,|p|p.palette.clone())),counts:previous.map_or_else(Vec::new,|p|p.counts.clone()),color_indices:previous.map_or_else(Vec::new,|p|p.color_indices.clone())}}
    }
    pub fn snapshot(&self)->&SegmentationSnapshot {&self.state}
    pub fn into_snapshot(self)->SegmentationSnapshot {self.state}
    pub fn run(&mut self,raster:Raster<'_>,parameters:&mut Value,palette:Option<&[f32]>,on_progress:&mut dyn FnMut(SegmentationProgress,&SegmentationSnapshot)->Result<(),String>)->Result<&SegmentationSnapshot,String>{
        let profile=format!("segmentation-{}-{}",if palette.is_some(){"fixed"}else{"unlimited"},if parameters["Shared::is_anti_aliased"].as_f64()==Some(1.0){"antialiased"}else{"aliased"});
        let ranges=&PROGRESS["profiles"][&profile];let mut previous=None;
        let state=&mut self.state;
        let mut checkpoint=|phase:i32,substep:i32,counter:i32,updated:ProgressUpdate<'_>|->Result<(),String>{
            if let Some(v)=updated.indices{state.indices=Some(v.to_vec());}if let Some(v)=updated.palette{state.palette=v.to_vec();}if let Some(v)=updated.counts{state.counts=v.to_vec();}if let Some(v)=updated.color_indices{state.color_indices=v.to_vec();}
            if previous==Some((phase,substep,counter)){return Ok(());}previous=Some((phase,substep,counter));
            let key=format!("{phase}:{substep}");let range=ranges[&key].as_array().ok_or_else(||format!("Unrecovered segmentation progress range: {profile}/{key}."))?;
            let (start,end,lower,upper)=(range[0].as_f64().unwrap(),range[1].as_f64().unwrap(),range[2].as_f64().unwrap(),range[3].as_f64().unwrap());
            let count=counter as f64;let fraction=if count<=lower{start}else if count>=upper{end}else{(end-start)*((count-lower)/(upper-lower))+start};
            on_progress(SegmentationProgress{phase,substep,counter,fraction,percent:(fraction*100.0+0.5).floor() as i32,minimum_pixels:updated.minimum_pixels},state)
        };
        checkpoint(2,-1,0,ProgressUpdate::default())?;
        segment_with_moments(raster,raster,parameters,palette,Some(&mut checkpoint))?;
        Ok(&self.state)
    }
}
