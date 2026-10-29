//! Directed subpixel blend-energy optimization and shadow refinement.
use std::collections::{BTreeSet,HashSet};
use serde_json::Value;
use crate::blend_solver::{EIGHT_DIRECTIONS,TWENTY_FOUR_DIRECTIONS,collect_blend_indices,write_blend_sample,fit_prepared_blend,dot_float};
use crate::region_data::{Raster,rebuild_connected_regions,repair_diagonal_map,palette_from_byte_colors};
use crate::segmentation::{ProgressSink,ProgressUpdate,publish,counter,nonnegative,parameter};

const CONTROL_NAMES:[&str;3]=["SubPixelSegmentation::self_weight_eps","SubPixelSegmentation::self_weight_penalty_low","SubPixelSegmentation::self_weight_penalty_high"];
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Neighborhood { Eight, TwentyFour }
impl Neighborhood {fn directions(self)->&'static [(i64,i64)] {match self {Self::Eight=>&EIGHT_DIRECTIONS,Self::TwentyFour=>&TWENTY_FOUR_DIRECTIONS}}}
#[derive(Debug,Clone,PartialEq)]
pub struct SubpixelRegion {
    pub count:u32,pub sums:[f32;4],pub color_index:i32,pub squares:[f32;4],pub interior_count:u32,
    pub interior_sums:[f32;4],pub interior_squares:[f32;4],pub color:[f32;4],pub boundary:Vec<usize>,pub interior_error:f64,pub error:f64,
}
#[derive(Debug,Clone,PartialEq)]
pub struct SubpixelState {pub indices:Vec<i32>,pub costs:Vec<f32>,pub regions:Vec<SubpixelRegion>}
fn controls(p:&Value)->Result<[f32;3],String>{Ok([parameter(p,CONTROL_NAMES[0])?,parameter(p,CONTROL_NAMES[1])?,parameter(p,CONTROL_NAMES[2])?])}
fn interior_error(s:&SubpixelRegion,color:&[f32;4])->f64 {
    if s.interior_count==0{return 0.0;}let count=s.interior_count as f64;let reciprocal=1.0/count;let mut error=0.0;
    for c in 0..4 {let mean=s.interior_sums[c] as f64*reciprocal;let mut variance=s.interior_squares[c] as f64-count*(mean*mean);if c<3{variance=variance.max(0.0);}let difference=color[c] as f64-mean;error+=(difference*difference)*count+variance;}error
}
fn refresh_region(s:&mut SubpixelRegion,costs:&[f32])->f64 {s.interior_error=interior_error(s,&s.color);let mut boundary=0.0;for &pixel in &s.boundary {boundary+=costs[pixel] as f64;}s.error=s.interior_error+boundary;s.error}
fn refresh_errors(state:&mut SubpixelState){for s in &mut state.regions {refresh_region(s,&state.costs);}}
fn pixel_cost(r:Raster<'_>,state:&SubpixelState,directions:&[(i64,i64)],settings:[f32;3],pixel:usize,source:i32,target:i32)->Result<f64,String>{
    let mut sample=[0.0;4];write_blend_sample(r.bgra,pixel*4,&mut sample);
    let selected=collect_blend_indices(&state.indices,r.width,r.height,pixel,directions,source,target);
    let mut colors=[0.0f32;100];
    for (entry,&neighbor) in selected.iter().enumerate(){colors[entry*4..entry*4+4].copy_from_slice(&state.regions[neighbor as usize].color);}
    let fitted=fit_prepared_blend(&sample,&colors[..selected.len()*4]).map_err(str::to_owned)?;
    let differences:[f32;4]=std::array::from_fn(|c|sample[c] as f32-fitted.color[c]);
    let mut penalty=(1.0-fitted.self_weight).max(0.0)*settings[1] as f64;
    penalty+=(settings[0] as f64-fitted.self_weight).max(0.0)*settings[2] as f64;
    Ok(penalty+dot_float(&differences,&differences) as f64)
}
fn emit(sink:&mut Option<&mut ProgressSink<'_>>,substep:i32,count:i32,indices:&[i32])->Result<(),String>{publish(sink,4,substep,count,ProgressUpdate{indices:Some(indices),..Default::default()})}
fn build(r:Raster<'_>,labels:&[i32],neighborhood:Neighborhood,settings:[f32;3],cached:Option<Vec<f32>>,sink:&mut Option<&mut ProgressSink<'_>>,substep:Option<i32>)->Result<SubpixelState,String>{
    let directions=neighborhood.directions();
    if let Some(step)=substep{emit(sink,step,0,labels)?;}
    let (indices,statistics)=rebuild_connected_regions(r.width,r.bgra,labels,None);
    if let Some(step)=substep{emit(sink,step,1,&indices)?;emit(sink,step,2,&indices)?;}
    let mut regions:Vec<SubpixelRegion>=statistics.into_iter().map(|s|SubpixelRegion{count:s.count,sums:s.sums,color_index:s.color_index,squares:[0.0;4],interior_count:0,interior_sums:[0.0;4],interior_squares:[0.0;4],color:[0.0;4],boundary:Vec::new(),interior_error:0.0,error:0.0}).collect();
    let mut boundary_sets=vec![BTreeSet::new();regions.len()];
    let mut interior_bytes:Vec<[Vec<u8>;4]>=(0..regions.len()).map(|_|std::array::from_fn(|_|Vec::new())).collect();
    if let Some(step)=substep{emit(sink,step,3,&indices)?;}
    for pixel in 0..indices.len(){
        if pixel&8191==0{if let Some(step)=substep{emit(sink,step+1,counter(pixel,indices.len()),&indices)?;}}
        let region=indices[pixel] as usize;let stat=&mut regions[region];let (x,y)=((pixel%r.width) as i64,(pixel/r.width) as i64);let mut boundary=false;
        for &(dx,dy) in directions{let (column,row)=(x+dx,y+dy);if column<0||row<0||column>=r.width as i64||row>=r.height as i64{continue;}let neighbor=row as usize*r.width+column as usize;if indices[neighbor]!=region as i32{boundary_sets[region].insert(neighbor);boundary=true;}}
        if boundary{boundary_sets[region].insert(pixel);}else{stat.interior_count+=1;}
        for c in 0..4{let byte=r.bgra[pixel*4+c];let value=byte as f32*(1.0f32/255.0);let square=value*value;stat.squares[c]+=square;if !boundary{stat.interior_sums[c]+=value;stat.interior_squares[c]+=square;interior_bytes[region][c].push(byte);}}
    }
    if let Some(step)=substep{emit(sink,step+2,0,&indices)?;}
    for (i,stat) in regions.iter_mut().enumerate(){
        if stat.interior_count<3{let reciprocal=1.0f32/stat.count as f32;for c in 0..4{stat.color[c]=stat.sums[c]*reciprocal;}}
        else{for c in 0..4{let bytes=&mut interior_bytes[i][c];bytes.sort_unstable();stat.color[c]=bytes[bytes.len()/2] as f32*(1.0f32/255.0);}}
        stat.boundary=std::mem::take(&mut boundary_sets[i]).into_iter().collect();
    }
    if let Some(step)=substep{emit(sink,step+2,1,&indices)?;}
    let costs=cached.unwrap_or_else(||vec![0.0;indices.len()]);let mut state=SubpixelState{indices,costs,regions};
    for region in 0..state.regions.len(){for i in 0..state.regions[region].boundary.len(){let pixel=state.regions[region].boundary[i];if state.indices[pixel]==region as i32{state.costs[pixel]=pixel_cost(r,&state,directions,settings,pixel,-1,-1)? as f32;}}}
    refresh_errors(&mut state);if let Some(step)=substep{emit(sink,step+2,2,&state.indices)?;}Ok(state)
}
pub fn build_subpixel_statistics(width:i64,height:i64,bgra:&[u8],labels:&[i64],parameters:&Value,neighborhood:Neighborhood)->Result<SubpixelState,String>{let settings=controls(parameters)?;let (r,labels)=crate::region_data::read_region_input(width,height,bgra,labels).map_err(|e|e.0)?;build(r,&labels,neighborhood,settings,None,&mut None,None)}
#[derive(Debug,Clone)]
pub struct SubpixelModel{width:usize,height:usize,bgra:Vec<u8>,neighborhood:Neighborhood,settings:[f32;3],state:SubpixelState}
impl SubpixelModel{
    pub fn new(width:i64,height:i64,bgra:&[u8],labels:&[i64],parameters:&Value,neighborhood:Neighborhood)->Result<Self,String>{let (r,labels)=crate::region_data::read_region_input(width,height,bgra,labels).map_err(|e|e.0)?;Self::create(r,&labels,parameters,neighborhood,&mut None,None)}
    fn create(r:Raster<'_>,labels:&[i32],parameters:&Value,neighborhood:Neighborhood,sink:&mut Option<&mut ProgressSink<'_>>,step:Option<i32>)->Result<Self,String>{
        let settings=controls(parameters)?;
        crate::region_data::read_raster_input(r.width as i64,r.height as i64,r.bgra).map_err(|e|e.0)?;
        if labels.len()!=r.width*r.height{return Err("Optimizer needs one region label per pixel.".into());}
        if labels.iter().any(|&v|v<0){return Err("Optimizer labels must be nonnegative int32 values.".into());}
        Ok(Self{width:r.width,height:r.height,bgra:r.bgra.to_vec(),neighborhood,settings,state:build(r,labels,neighborhood,settings,None,sink,step)?})
    }
    fn raster(&self)->Raster<'_>{Raster{width:self.width,height:self.height,bgra:&self.bgra}}
    pub fn snapshot(&mut self)->SubpixelState{refresh_errors(&mut self.state);self.state.clone()}
    fn into_snapshot(mut self)->SubpixelState{refresh_errors(&mut self.state);self.state}
    pub fn region_count(&self)->usize{self.state.regions.len()}
    pub fn rebuild(&mut self,neighborhood:Neighborhood)->Result<(),String>{self.rebuild_with_progress(neighborhood,&mut None,None)}
    fn rebuild_with_progress(&mut self,neighborhood:Neighborhood,sink:&mut Option<&mut ProgressSink<'_>>,step:Option<i32>)->Result<(),String>{self.neighborhood=neighborhood;let cached=std::mem::take(&mut self.state.costs);self.state=build(self.raster(),&self.state.indices,neighborhood,self.settings,Some(cached),sink,step)?;Ok(())}
    pub fn repair_diagonal_contacts(&mut self){repair_diagonal_map(self.width,self.height,&self.bgra,&mut self.state.indices);}
    pub fn project_palette(&mut self,palette:&[f64],minimum_interior_pixels:i64)->Result<(),String>{
        if !(0..=i32::MAX as i64).contains(&minimum_interior_pixels){return Err("Palette projection needs a nonnegative int32 minimumInteriorPixels.".into());}
        let colors=crate::pixel_input::read_palette_colors(palette).map_err(|e|e.0)?;self.project_colors(&colors,minimum_interior_pixels as u32);Ok(())
    }
    fn project_colors(&mut self,palette:&[f32],minimum:u32){for stat in &mut self.state.regions{if stat.color_index>=0||stat.interior_count<minimum||stat.count==0{continue;}let mut best=0;let mut best_cost=f64::INFINITY;for color in 0..palette.len()/4{let mut cost=0.0;for c in 0..4{let difference=(stat.color[c]-palette[color*4+c]) as f64;cost+=difference*difference;}if cost<best_cost{best=color;best_cost=cost;}}stat.color.copy_from_slice(&palette[best*4..best*4+4]);stat.color_index=best as i32;}}
    fn pair(&self,source:usize,target:usize)->Result<(),String>{if source>=self.state.regions.len()||target>=self.state.regions.len()||self.state.regions[source].count==0||self.state.regions[target].count==0{Err("Merge source and target must be active region identifiers in the model.".into())}else{Ok(())}}
    pub fn merge_cost(&mut self,source:usize,target:usize,lambda:f64)->Result<f64,String>{self.pair(source,target)?;let lambda=nonnegative(lambda,"Merge lambda")?;self.cost(source,target,lambda)}
    fn cost(&mut self,source:usize,target:usize,lambda:f32)->Result<f64,String>{let color=self.state.regions[target].color;let stat=&mut self.state.regions[source];let lower=interior_error(stat,&color)-refresh_region(stat,&self.state.costs);if lower>lambda as f64{return Ok(lower);}let mut boundary=0.0;for &pixel in &self.state.regions[source].boundary{boundary+=pixel_cost(self.raster(),&self.state,self.neighborhood.directions(),self.settings,pixel,source as i32,target as i32)? as f32 as f64;}Ok(lower+boundary)}
    pub fn boundary_cost(&self,source:usize,target:usize,budget:f64)->Result<f64,String>{self.pair(source,target)?;if !budget.is_finite()||budget<0.0{return Err("Boundary budget must be a finite nonnegative number.".into());}self.bounded_cost(source,target,budget)}
    fn bounded_cost(&self,source:usize,target:usize,budget:f64)->Result<f64,String>{let mut error=0.0;for &pixel in &self.state.regions[source].boundary{error+=pixel_cost(self.raster(),&self.state,self.neighborhood.directions(),self.settings,pixel,source as i32,target as i32)?;if error>budget{break;}}Ok(error)}
    pub fn select_colors(&mut self,margin:f64)->Result<(),String>{self.select_with_progress(margin,&mut None,None)}
    fn select_with_progress(&mut self,margin:f64,sink:&mut Option<&mut ProgressSink<'_>>,step:Option<i32>)->Result<(),String>{
        let margin=nonnegative(margin,"Color selection margin")?;let mut candidates:Vec<usize>=self.state.regions.iter().enumerate().filter(|(_,s)|s.interior_count>=3).map(|(i,_)|i).collect();
        candidates.sort_unstable_by(|&a,&b|self.state.regions[b].interior_count.cmp(&self.state.regions[a].interior_count).then(b.cmp(&a)));candidates.truncate(100);
        if let Some(step)=step{emit(sink,step,0,&self.state.indices)?;}
        for source in 0..self.state.regions.len(){if source&3==0{if let Some(step)=step{emit(sink,step,counter(source,self.region_count().saturating_sub(1)),&self.state.indices)?;}}if self.state.regions[source].interior_count>=3{continue;}let mut best=refresh_region(&mut self.state.regions[source],&self.state.costs)-margin as f64;let mut selected=source;
            for &target in &candidates{let interior=interior_error(&self.state.regions[source],&self.state.regions[target].color);if !(interior<best){continue;}let error=interior+self.bounded_cost(source,target,best-interior)?;if error<best{best=error;selected=target;}}
            if selected!=source{self.state.regions[source].color=self.state.regions[selected].color;self.state.regions[source].color_index=self.state.regions[selected].color_index;}
        }Ok(())
    }
    pub fn merge(&mut self,source:usize,target:usize)->Result<(),String>{self.pair(source,target)?;if source==target{return Err("An applied merge needs distinct source and target regions.".into());}self.apply_merge(source,target)}
    fn apply_merge(&mut self,source:usize,target:usize)->Result<(),String>{
        for label in &mut self.state.indices{if *label==source as i32{*label=target as i32;}}
        let source_boundary=std::mem::take(&mut self.state.regions[source].boundary);
        let (from,into)=if source<target{let(left,right)=self.state.regions.split_at_mut(target);(&mut left[source],&mut right[0])}else{let(left,right)=self.state.regions.split_at_mut(source);(&mut right[0],&mut left[target])};
        into.count+=from.count;into.interior_count+=from.interior_count;
        for c in 0..4{into.sums[c]+=from.sums[c];into.squares[c]+=from.squares[c];into.interior_sums[c]+=from.interior_sums[c];into.interior_squares[c]+=from.interior_squares[c];}
        from.count=0;from.sums=[0.0;4];from.interior_count=0;from.interior_sums=[0.0;4];from.interior_squares=[0.0;4];
        for &pixel in &source_boundary{self.state.costs[pixel]=pixel_cost(self.raster(),&self.state,self.neighborhood.directions(),self.settings,pixel,-1,-1)? as f32;}
        let mut boundary:BTreeSet<usize>=self.state.regions[target].boundary.iter().copied().collect();
        for &pixel in &source_boundary{if boundary.insert(pixel){continue;}if self.state.indices[pixel]!=target as i32{continue;}let (x,y)=((pixel%self.width) as i64,(pixel/self.width) as i64);let mut interior=true;
            for &(dx,dy) in self.neighborhood.directions(){let (column,row)=(x+dx,y+dy);if column>=0&&row>=0&&column<self.width as i64&&row<self.height as i64&&self.state.indices[row as usize*self.width+column as usize]!=target as i32{interior=false;break;}}
            if !interior{continue;}let into=&mut self.state.regions[target];into.interior_count+=1;for c in 0..4{let value=self.bgra[pixel*4+c] as f32*(1.0f32/255.0);into.interior_sums[c]+=value;into.interior_squares[c]+=value*value;}boundary.remove(&pixel);
        }
        self.state.regions[target].boundary=boundary.into_iter().collect();self.state.regions[source].boundary.clear();Ok(())
    }
    pub fn iterate(&mut self,lambda:f64,maximum_source_pixels:i64,maximum_target_pixels:i64,source_interior:bool)->Result<usize,String>{
        let threshold=nonnegative(lambda,"Pass lambda")?;if !(0..=i32::MAX as i64).contains(&maximum_source_pixels)||!(0..=i32::MAX as i64).contains(&maximum_target_pixels){return Err("Pass source and target size limits must be nonnegative int32 values.".into());}
        let mut examined=HashSet::new();let mut merges=0;
        for source in 0..self.state.regions.len(){let stat=&self.state.regions[source];let count=if source_interior{stat.interior_count}else{stat.count};if stat.count==0||count>maximum_source_pixels as u32{continue;}let mut best_target=None;let mut best_cost=threshold as f64;
            for i in 0..self.state.regions[source].boundary.len(){let pixel=self.state.regions[source].boundary[i];let target=self.state.indices[pixel] as usize;let pair=((source as u32)<<16)|(target as u32);if target==source||examined.contains(&pair)||self.state.regions[target].count>maximum_target_pixels as u32{continue;}let cost=self.cost(source,target,threshold)?;if cost<best_cost{best_cost=cost;best_target=Some(target);}examined.insert(pair);}
            if let Some(target)=best_target{self.apply_merge(source,target)?;merges+=1;}
        }Ok(merges)
    }
    fn run_shadow_stage(&mut self,source:Raster<'_>,parameters:&Value,palette:Option<&[f32]>,sink:&mut Option<&mut ProgressSink<'_>>)->Result<(),String>{
        let transferred=if parameters["clusterColors"].as_bool()==Some(true)||palette.is_some(){let (colors,indices)=transfer_subpixel_palette(&self.state,palette);self.state.indices=rebuild_connected_regions(self.width,&self.bgra,&indices,None).0;Some(colors)}else{None};
        self.neighborhood=Neighborhood::Eight;let cached=std::mem::take(&mut self.state.costs);
        {let mut forwarding=|phase,substep,count,update:ProgressUpdate<'_>|publish(sink,phase,substep,count,ProgressUpdate{indices:update.indices,counts:update.counts,color_indices:update.color_indices,palette:transferred.as_deref().or(update.palette),minimum_pixels:update.minimum_pixels});
        self.state=build(self.raster(),&self.state.indices,self.neighborhood,self.settings,Some(cached),&mut Some(&mut forwarding),Some(27))?;}
        emit(sink,30,0,&self.state.indices)?;let blurred=crate::image_classification::blur_classifier_pixels(source.width,source.height,source.bgra);emit(sink,30,1,&self.state.indices)?;
        let gradients=crate::image_classification::measure_classifier_gradients(self.width,self.height,&self.bgra,&blurred,source.width);emit(sink,30,2,&self.state.indices)?;
        let regions:Vec<_>=self.state.regions.iter().map(|s|crate::shadow_classification::ShadowRegion{count:s.count,interior_count:s.interior_count,sums:s.sums,squares:s.squares,color:s.color,boundary:s.boundary.clone()}).collect();
        let gradient:Vec<f64>=gradients.gradient.iter().map(|&v|v as f64).collect();let blur_gradient:Vec<f64>=gradients.blur_gradient.iter().map(|&v|v as f64).collect();
        let features=crate::shadow_classification::measure_shadow_features(self.width,self.height,&self.bgra,&self.state.indices,&regions,&gradient,&blur_gradient,0.0).features;
        emit(sink,30,3,&self.state.indices)?;let flags:Vec<u8>=features.iter().map(|f|crate::shadow_classification::classify_shadow_features(f) as u8).collect();emit(sink,30,4,&self.state.indices)?;emit(sink,31,0,&self.state.indices)?;emit(sink,31,1,&self.state.indices)?;
        self.state=refine_shadow_state_with_progress(self.raster(),&self.state,&features,&flags,parameters,parameters["shadowControl"].as_f64().unwrap() as i32,sink)?.state;Ok(())
    }
}
fn optimize_model(r:Raster<'_>,labels:&[i32],p:&Value,sink:&mut Option<&mut ProgressSink<'_>>)->Result<SubpixelModel,String>{
    let lambda=parameter(p,"SubPixelSegmenter::lambda_f")?;let wide=match p["SubPixelSegmenter::use_connected_24_iteration"].as_f64(){Some(0.0)=>false,Some(1.0)=>true,_=>return Err("SubPixelSegmenter::use_connected_24_iteration must be 0 or 1.".into())};
    if p.get("clusterColors").is_some_and(|v|!v.is_boolean()){return Err("clusterColors must be a boolean when supplied.".into());}
    emit(sink,-1,0,labels)?;let mut model=SubpixelModel::create(r,labels,p,Neighborhood::Eight,sink,Some(0))?;let max=i32::MAX as i64;
    emit(sink,3,0,&model.state.indices)?;let mut changes=model.iterate(lambda as f64,1,1,false)?;
    for i in 0..20{if changes==0{break;}emit(sink,3,counter(i,20),&model.state.indices)?;changes=model.iterate(lambda as f64,(i+2) as i64,(i+2) as i64,false)?;}
    model.rebuild_with_progress(Neighborhood::Eight,sink,Some(4))?;emit(sink,7,0,&model.state.indices)?;changes=model.iterate(lambda as f64,1,max,false)?;
    for i in 0..20{if changes==0{break;}emit(sink,7,counter(i,20),&model.state.indices)?;changes=model.iterate(lambda as f64,(i+2) as i64,max,false)?;}
    emit(sink,8,0,&model.state.indices)?;changes=model.iterate(lambda as f64,max,max,false)?;
    for i in 0..20{if changes==0{break;}emit(sink,8,counter(i,20),&model.state.indices)?;changes=model.iterate(lambda as f64,max,max,false)?;}
    if wide{model.rebuild_with_progress(Neighborhood::TwentyFour,sink,Some(9))?;emit(sink,12,0,&model.state.indices)?;changes=model.iterate(lambda as f64,20,max,true)?;for i in 0..20{if changes==0{break;}emit(sink,12,counter(i,20),&model.state.indices)?;changes=model.iterate(lambda as f64,20,max,true)?;}model.rebuild_with_progress(Neighborhood::Eight,sink,Some(13))?;}
    for region in (0..model.region_count()).step_by(4){emit(sink,16,counter(region,model.region_count().saturating_sub(1)),&model.state.indices)?;}
    model.repair_diagonal_contacts();emit(sink,17,0,&model.state.indices)?;model.rebuild_with_progress(Neighborhood::Eight,sink,Some(18))?;
    emit(sink,21,0,&model.state.indices)?;changes=model.iterate(lambda as f64,4,max,false)?;for i in 0..20{if changes==0{break;}emit(sink,21,counter(i,20),&model.state.indices)?;changes=model.iterate(lambda as f64,4,max,false)?;}
    model.rebuild_with_progress(Neighborhood::Eight,sink,Some(22))?;emit(sink,25,0,&model.state.indices)?;Ok(model)
}
fn select_colors(model:&mut SubpixelModel,p:&Value,palette:Option<&[f32]>,sink:&mut Option<&mut ProgressSink<'_>>)->Result<(),String>{if p["clusterColors"].as_bool()==Some(true)||palette.is_some(){model.select_with_progress(p["SubPixelSegmenter::color_cluster_margin"].as_f64().unwrap_or(f64::NAN),sink,Some(46))?;}if let Some(palette)=palette{model.project_colors(palette,0);}Ok(())}
pub fn optimize_subpixel(r:Raster<'_>,labels:&[i32],parameters:&Value,palette:Option<&[f32]>)->Result<SubpixelState,String>{if let Some(palette)=palette{crate::segmentation::validate_palette(palette)?;}let mut model=optimize_model(r,labels,parameters,&mut None)?;select_colors(&mut model,parameters,palette,&mut None)?;Ok(model.into_snapshot())}
pub fn segment_subpixel_state(r:Raster<'_>,source:Raster<'_>,labels:&[i32],parameters:&Value,palette:Option<&[f32]>,sink:&mut Option<&mut ProgressSink<'_>>)->Result<SubpixelState,String>{
    if let Some(palette)=palette{crate::segmentation::validate_palette(palette)?;}
    if parameters.get("shadowing").is_some_and(|v|!v.is_boolean()){return Err("shadowing must be a boolean when supplied.".into());}
    let shadow=parameters["shadowing"].as_bool()==Some(true);
    if shadow{let control=parameters["shadowControl"].as_f64().unwrap_or(f64::NAN);if !control.is_finite()||control.fract()!=0.0||control<i32::MIN as f64||control>i32::MAX as f64{return Err("Enabled shadowing requires an explicit int32 shadowControl.".into());}if r.width<2||r.height<2{return Err("Shadowing requires a prepared image of at least 2 by 2.".into());}}
    let mut model=optimize_model(r,labels,parameters,sink)?;
    if shadow{if let Some(palette)=palette{model.project_colors(palette,0);}else if parameters["clusterColors"].as_bool()==Some(true){model.select_with_progress(parameters["SubPixelSegmenter::color_cluster_margin"].as_f64().unwrap_or(f64::NAN),sink,Some(26))?;}model.run_shadow_stage(source,parameters,palette,sink)?;}
    select_colors(&mut model,parameters,palette,sink)?;Ok(model.into_snapshot())
}
pub fn transfer_subpixel_palette(state:&SubpixelState,supplied:Option<&[f32]>)->(Vec<f32>,Vec<i32>){
    let (palette,ids)=if let Some(palette)=supplied{(palette.to_vec(),state.regions.iter().map(|s|s.color_index).collect())}else{let bytes:Vec<[u8;4]>=state.regions.iter().map(|s|s.color.map(|v|((v as f64*255.0+0.5).trunc() as i64&255) as u8)).collect();palette_from_byte_colors(&bytes)};
    (palette,state.indices.iter().map(|&id|ids[id as usize]).collect())
}
#[derive(Debug,Clone,PartialEq)]
pub struct ShadowRefinement{pub state:SubpixelState,pub features:Vec<[f64;42]>,pub accepted:Vec<u8>,pub assignments:Vec<crate::shadow_classification::ShadowAssignment>,pub before:Option<SubpixelState>,pub after:Option<SubpixelState>}
pub fn refine_shadow_state(r:Raster<'_>,prepared:&SubpixelState,features:&[[f64;42]],flags:&[u8],parameters:&Value,control:i32)->Result<ShadowRefinement,String>{refine_shadow_state_with_progress(r,prepared,features,flags,parameters,control,&mut None)}
fn refine_shadow_state_with_progress(r:Raster<'_>,prepared:&SubpixelState,features:&[[f64;42]],flags:&[u8],parameters:&Value,control:i32,sink:&mut Option<&mut ProgressSink<'_>>)->Result<ShadowRefinement,String>{
    let settings=controls(parameters)?;let mut measured=features.to_vec();let mut accepted=vec![0;flags.len()];let shadow:Vec<usize>=flags.iter().enumerate().filter(|(_,f)|**f&1!=0).map(|(i,_)|i).collect();
    if shadow.is_empty(){return Ok(ShadowRefinement{state:prepared.clone(),features:measured,accepted,assignments:Vec::new(),before:None,after:None});}
    let before=build(r,&prepared.indices,Neighborhood::TwentyFour,settings,Some(prepared.costs.clone()),sink,Some(32))?;emit(sink,35,0,&before.indices)?;
    let mut old_energies=Vec::new();let mut pixels_by_shadow=Vec::new();let mut outside_by_shadow=Vec::new();
    for (ordinal,&region) in shadow.iter().enumerate(){let stat=&before.regions[region];pixels_by_shadow.push(before.indices.iter().enumerate().filter(|(_,v)|**v==region as i32).map(|(i,_)|i).collect::<Vec<_>>());outside_by_shadow.push(stat.boundary.iter().copied().filter(|&p|before.indices[p]!=region as i32).collect::<Vec<_>>());let mut old=before.regions[ordinal].interior_error as f32;for &pixel in &stat.boundary{old+=before.costs[pixel];}old_energies.push(old);}
    emit(sink,35,1,&before.indices)?;emit(sink,35,2,&before.indices)?;
    let (filled,assignments)=crate::shadow_classification::reassign_shadow_pixels(r.width,r.height,r.bgra,&before.indices,flags)?;emit(sink,35,3,&filled)?;emit(sink,35,4,&filled)?;
    let eight=build(r,&filled,Neighborhood::Eight,settings,None,sink,Some(36))?;let after=build(r,&eight.indices,Neighborhood::TwentyFour,settings,Some(eight.costs),sink,Some(39))?;emit(sink,42,0,&after.indices)?;
    for (ordinal,&region) in shadow.iter().enumerate(){let pixels=&pixels_by_shadow[ordinal];let outside=&outside_by_shadow[ordinal];let old=old_energies[ordinal];let mut new=0.0f32;for &pixel in pixels.iter().chain(outside.iter()){new=(new as f64+pixel_cost(r,&after,Neighborhood::TwentyFour.directions(),settings,pixel,-1,-1)?) as f32;}let count=(pixels.len()+outside.len()) as f32;let reciprocal=1.0f32/count;let values=&mut measured[region];let difference=new as f64-old as f64;values[33]=old as f64;values[34]=new as f64;values[35]=reciprocal as f64*old as f64;values[36]=reciprocal as f64*new as f64;values[37]=difference;values[38]=difference/count as f64;}
    let mut indices=filled;emit(sink,42,1,&indices)?;
    for (ordinal,&region) in shadow.iter().enumerate(){if crate::shadow_classification::accept_shadow_features(&measured[region],control){accepted[region]=1;}else{for &pixel in &pixels_by_shadow[ordinal]{indices[pixel]=region as i32;}}}
    emit(sink,42,2,&indices)?;let connected=rebuild_connected_regions(r.width,r.bgra,&indices,None).0;let state=build(r,&connected,Neighborhood::Eight,settings,None,sink,Some(43))?;
    Ok(ShadowRefinement{state,features:measured,accepted,assignments,before:Some(before),after:Some(after)})
}
