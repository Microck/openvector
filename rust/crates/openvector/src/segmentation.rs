//! Native region initialization, optimization and anti-alias dispatch.
use crate::region_data::{Raster, RegionStatistic, rebuild_connected_regions, repair_diagonal_map, palette_from_byte_colors};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct RegionSnapshot {
    pub indices: Vec<i32>, pub counts: Vec<i32>, pub sums: Vec<f32>,
    pub color_indices: Vec<i32>, pub active_count: usize,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentResult {
    pub indices: Vec<i32>, pub counts: Vec<i32>, pub sums: Vec<f32>,
    pub palette: Vec<f32>, pub color_indices: Vec<i32>,
}
#[derive(Default, Clone, Copy)]
pub struct ProgressUpdate<'a> {
    pub indices: Option<&'a [i32]>, pub counts: Option<&'a [i32]>,
    pub palette: Option<&'a [f32]>, pub color_indices: Option<&'a [i32]>,
    pub minimum_pixels: Option<u32>,
}
pub type ProgressSink<'a> = dyn FnMut(i32, i32, i32, ProgressUpdate<'_>) -> Result<(), String> + 'a;
pub(crate) fn publish(sink: &mut Option<&mut ProgressSink<'_>>, phase: i32, substep: i32, counter: i32, update: ProgressUpdate<'_>) -> Result<(), String> {
    if let Some(sink) = sink.as_deref_mut() { sink(phase, substep, counter, update)?; }
    Ok(())
}
pub(crate) fn counter(value: usize, total: usize) -> i32 {
    if total == 0 { return 0; }
    (((value as i32).wrapping_mul(50) as f64 / total as f64 + 0.5).trunc()) as i32
}
pub(crate) fn nonnegative(value: f64, name: &str) -> Result<f32, String> {
    if !value.is_finite() || value < 0.0 || !(value as f32).is_finite() { return Err(format!("{name} must be a finite nonnegative float32 number.")); }
    Ok(value as f32)
}
pub(crate) fn parameter(parameters: &Value, name: &str) -> Result<f32, String> {
    nonnegative(parameters[name].as_f64().unwrap_or(f64::NAN), name)
}
fn norm(v: [f32; 4]) -> f32 { ((v[0]*v[0] + v[1]*v[1]) + v[2]*v[2]) + v[3]*v[3] }
fn merge_cost(a: u32, first: &[f32; 4], b: u32, second: &[f32; 4]) -> f32 {
    let merged = std::array::from_fn(|c| first[c]+second[c]);
    (norm(*first)/a as f32 + norm(*second)/b as f32) - norm(merged)/(a+b) as f32
}
pub fn region_color_merge_cost(counts: [i64; 2], sums: [[f64; 4]; 2]) -> Result<f32, String> {
    if counts.iter().any(|&v| v < 1 || v > i32::MAX as i64) || counts[0]+counts[1] > i32::MAX as i64 { return Err("Merge cost needs two positive int32 counts whose sum fits int32.".into()); }
    if sums.iter().flatten().any(|v| !v.is_finite() || !(*v as f32).is_finite()) { return Err("Region sums must be finite float32 numbers.".into()); }
    Ok(merge_cost(counts[0] as u32, &sums[0].map(|v| v as f32), counts[1] as u32, &sums[1].map(|v| v as f32)))
}
#[derive(Default, Clone, Copy)]
struct Tile { sums: [f64; 4], squares: [f64; 4], count: usize, cost: f64 }
fn paint(labels: &mut [i32], width: usize, height: usize, x: usize, y: usize, size: usize, next: &mut i32) {
    for row in y..height.min(y+size) { for column in x..width.min(x+size) { labels[row*width+column] = *next; } }
    *next = next.wrapping_add(1);
}
fn tile(r: Raster<'_>, labels: &mut [i32], next: &mut i32, x: usize, y: usize, size: usize, top: bool, threshold: f32) -> Tile {
    let mut out = Tile::default();
    if x >= r.width || y >= r.height { return out; }
    if size == 1 {
        for c in 0..4 { out.sums[c] = r.bgra[(y*r.width+x)*4+c] as f64 * (1.0/255.0); out.squares[c] = out.sums[c]*out.sums[c]; }
        out.count = 1; return out;
    }
    let half = size/2;
    let quadrants = [(x,y),(x+half,y),(x,y+half),(x+half,y+half)];
    let mut children = [Tile::default(); 4];
    for (i, &(column,row)) in quadrants.iter().enumerate() {
        children[i] = tile(r, labels, next, column, row, half, false, threshold);
        out.count += children[i].count;
        for c in 0..4 { out.sums[c] += children[i].sums[c]; out.squares[c] += children[i].squares[c]; }
    }
    let mut squared = 0.0; let mut length = 0.0;
    for c in 0..4 { squared += out.squares[c]; length += out.sums[c]*out.sums[c]; }
    out.cost = (squared-length/out.count as f64)/out.count as f64;
    if out.cost > threshold as f64 {
        for (i,&(column,row)) in quadrants.iter().enumerate() { if children[i].cost <= threshold as f64 { paint(labels,r.width,r.height,column,row,half,next); } }
    } else if top { paint(labels,r.width,r.height,x,y,size,next); }
    out
}
fn initialize(r: Raster<'_>, threshold: f32, sink: &mut Option<&mut ProgressSink<'_>>) -> Result<Vec<i32>,String> {
    let mut labels: Vec<i32> = (0..r.width*r.height).map(|v|v as i32).collect(); let mut next = labels.len() as i32;
    for y in (0..r.height).step_by(16) {
        publish(sink,3,0,counter(y,r.height),ProgressUpdate { indices:Some(&labels),..Default::default() })?;
        for x in (0..r.width).step_by(16) { tile(r,&mut labels,&mut next,x,y,16,true,threshold); }
    }
    Ok(labels)
}
pub fn initialize_regions(width: i64, height: i64, bgra: &[u8], threshold: f64) -> Result<Vec<i32>,String> {
    let r = crate::region_data::read_raster_input(width,height,bgra).map_err(|e|e.0)?;
    initialize(r,nonnegative(threshold,"Region threshold")?, &mut None)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerMode { Axis, Eight, Small }
#[derive(Debug, Clone)]
pub struct RegionOptimizer {
    width: usize, height: usize, bgra: Vec<u8>, indices: Vec<i32>, parents: Vec<i32>,
    statistics: Vec<RegionStatistic>, final_lambda: f32, minimum_pixels: u32, palette: Option<Vec<f32>>,
}
impl RegionOptimizer {
    pub fn new(width: i64, height: i64, bgra: &[u8], regions: &[i64], final_lambda: f64, minimum_pixels: i64) -> Result<Self,String> {
        let (r, labels) = crate::region_data::read_region_input(width,height,bgra,regions).map_err(|e|e.0)?;
        if !(0..=i32::MAX as i64).contains(&minimum_pixels) { return Err("minimumPixels must be a nonnegative int32 value.".into()); }
        Ok(Self::owned(r,labels,nonnegative(final_lambda,"finalLambda")?,minimum_pixels as u32,None))
    }
    fn owned(r: Raster<'_>, indices: Vec<i32>, final_lambda: f32, minimum_pixels: u32, palette: Option<Vec<f32>>) -> Self {
        let statistics = palette.as_ref().map_or_else(Vec::new,|p| (0..p.len()/4).map(|i|RegionStatistic {count:0,sums:[0.0;4],color_index:i as i32}).collect());
        let mut out = Self {width:r.width,height:r.height,bgra:r.bgra.to_vec(),indices,parents:Vec::new(),statistics,final_lambda,minimum_pixels,palette};
        out.rebuild_connected(); out
    }
    fn rebuild_connected(&mut self) {
        let (indices,statistics) = rebuild_connected_regions(self.width,&self.bgra,&self.indices, self.palette.as_ref().map(|_|self.statistics.as_slice()));
        self.indices=indices; self.parents=vec![-1;statistics.len()]; self.statistics=statistics;
    }
    fn root(&mut self, pixel: usize) -> usize {
        let original = self.indices[pixel] as usize; let mut root=original;
        while self.parents[root]>=0 { root=self.parents[root] as usize; }
        if root!=original { self.parents[original]=root as i32; self.indices[pixel]=root as i32; }
        root
    }
    fn merge(&mut self,first:usize,second:usize) {
        self.parents[second]=first as i32; self.statistics[first].count+=self.statistics[second].count; self.statistics[second].count=0;
        for c in 0..4 { self.statistics[first].sums[c]+=self.statistics[second].sums[c]; self.statistics[second].sums[c]=0.0; }
    }
    fn move_pixel(&mut self,pixel:usize,first:usize,second:usize) {
        self.indices[pixel]=second as i32; self.statistics[first].count-=1;self.statistics[second].count+=1;
        for c in 0..4 {let value=self.bgra[pixel*4+c] as f32*(1.0f32/255.0); self.statistics[second].sums[c]+=value;self.statistics[first].sums[c]-=value;}
    }
    fn pixel_cost(&self,pixel:usize,region:usize)->f32 {
        let stat=&self.statistics[region];let scale=255.0f32/stat.count as f32;
        norm(std::array::from_fn(|c|self.bgra[pixel*4+c] as f32-stat.sums[c]*scale))
    }
    pub fn iterate(&mut self,lambda:f64,mode:OptimizerMode,minimum_pixels:Option<i64>)->Result<(),String> {
        let lambda=nonnegative(lambda,"lambda")?;
        let minimum=minimum_pixels.unwrap_or(self.minimum_pixels as i64);
        if !(0..=i32::MAX as i64).contains(&minimum) {return Err("Pass minimumPixels must be a nonnegative int32 value.".into());}
        let directions=if mode==OptimizerMode::Eight { &crate::blend_solver::EIGHT_DIRECTIONS[..] } else { &crate::blend_solver::EIGHT_DIRECTIONS[..2] };
        for y in 0..self.height { for x in 0..self.width {
            let pixel=y*self.width+x;let mut first=self.root(pixel);
            for &(dx,dy) in directions {
                let (column,row)=(x as i64+dx,y as i64+dy);
                if column<0||row<0||column>=self.width as i64||row>=self.height as i64 {continue;}
                let neighbor=row as usize*self.width+column as usize;let second=self.root(neighbor);if first==second {continue;}
                let (a,b)=(&self.statistics[first],&self.statistics[second]);
                let small=a.count<minimum as u32||b.count<minimum as u32;
                if mode==OptimizerMode::Small {if small {self.merge(first,second);}}
                else if merge_cost(a.count,&a.sums,b.count,&b.sums)<lambda || (lambda==self.final_lambda&&small) {self.merge(first,second);}
                else if self.pixel_cost(pixel,second)<self.pixel_cost(pixel,first) {self.move_pixel(pixel,first,second);}
                else if self.pixel_cost(neighbor,first)<self.pixel_cost(neighbor,second) {self.move_pixel(neighbor,second,first);}
                first=self.root(pixel);
            }
        }} Ok(())
    }
    pub fn rebuild(&mut self) {for pixel in 0..self.indices.len(){self.root(pixel);}self.rebuild_connected();}
    pub fn repair_diagonal_contacts(&mut self) {for pixel in 0..self.indices.len(){self.root(pixel);}repair_diagonal_map(self.width,self.height,&self.bgra,&mut self.indices);}
    pub fn project_palette(&mut self,diagonal:bool)->Result<Vec<f32>,String> {
        if self.palette.is_some(){return Err("Regions already have a projected palette.".into());}
        if self.statistics.iter().any(|s|s.count==0){return Err("Rebuild connected regions before projecting their palette.".into());}
        let colors:Vec<[u8;4]>=self.statistics.iter().map(|s|{
            let scale=(if diagonal{255.0}else{1.0})/s.count as f32;
            std::array::from_fn(|c|{let mean=s.sums[c]*scale;let value=if diagonal{mean as f64}else{mean as f64*255.0+0.5};(value.trunc() as i64&255) as u8})
        }).collect();
        let (palette,ids)=palette_from_byte_colors(&colors);
        for (s,&id) in self.statistics.iter_mut().zip(&ids){s.color_index=id;}
        self.palette=Some(palette.clone());
        if diagonal {
            for pixel in 0..self.indices.len(){let root=self.root(pixel);self.indices[pixel]=self.statistics[root].color_index;}
            self.statistics=(0..palette.len()/4).map(|i|RegionStatistic{count:0,sums:[0.0;4],color_index:i as i32}).collect();self.rebuild_connected();
        } Ok(palette)
    }
    pub fn labels(&mut self)->Vec<i32>{(0..self.indices.len()).map(|p|self.root(p) as i32).collect()}
    pub fn region_count(&self)->usize{self.statistics.len()}
    pub fn snapshot(&mut self)->RegionSnapshot {
        RegionSnapshot {indices:self.labels(),counts:self.statistics.iter().map(|s|s.count as i32).collect(),sums:self.statistics.iter().flat_map(|s|s.sums).collect(),color_indices:self.statistics.iter().map(|s|s.color_index).collect(),active_count:self.statistics.iter().filter(|s|s.count>0).count()}
    }
    fn publish(&self,sink:&mut Option<&mut ProgressSink<'_>>,phase:i32,substep:i32,counter:i32,palette:Option<&[f32]>)->Result<(),String>{publish(sink,phase,substep,counter,ProgressUpdate{indices:Some(&self.indices),palette,..Default::default()})}
}
struct Settings{iterations:usize,renumber:usize,minimum:u32,pre:f32,initial:f32,final_:f32,rise:f32,diagonal:bool,antialias:bool}
fn settings(p:&Value)->Result<Settings,String>{
    if p["Segmenter::size_penalty_weight"].as_f64()!=Some(0.0){return Err("Nonzero size penalties remain under reconstruction.".into());}
    let diagonal=match p["Segmenter::use_over_seg_diag_extr"].as_f64(){Some(0.0)=>false,Some(1.0)=>true,_=>return Err("Segmenter::use_over_seg_diag_extr must be 0 or 1.".into())};
    let mut ints=[0;3];for (i,name) in ["max_iterations","num_iter_btwn_renum","min_num_pixels"].iter().enumerate(){let value=p[format!("Segmenter::{name}")].as_f64().unwrap_or(f64::NAN);if !value.is_finite()||value.fract()!=0.0||value<(if i==2{0.0}else{1.0})||value>i32::MAX as f64{return Err(format!("Segmenter::{name} must be a valid {} int32 value.",if i==2{"nonnegative"}else{"positive"}));}ints[i]=value as usize;}
    let mut floats=[0.0;4];for (i,name) in ["lambda_pre","lambda_initial","lambda_final","rise_fraction"].iter().enumerate(){floats[i]=parameter(p,&format!("Segmenter::{name}"))?;if i>0&&floats[i]==0.0{return Err(format!("Segmenter::{name} must be positive."));}}
    Ok(Settings{iterations:ints[0],renumber:ints[1],minimum:ints[2] as u32,pre:floats[0],initial:floats[1],final_:floats[2],rise:floats[3],diagonal,antialias:p["Shared::is_anti_aliased"].as_f64()==Some(1.0)})
}
fn run_base(r:Raster<'_>,s:&Settings,diagonal:bool,p:&mut Value,sink:&mut Option<&mut ProgressSink<'_>>)->Result<(RegionOptimizer,f32),String>{
    let labels=initialize(r,s.pre,sink)?;let mut optimizer=RegionOptimizer::owned(r,labels,s.final_,s.minimum,None);
    let multiplier=((s.final_ as f64/s.initial as f64).ln()/(s.iterations as f64*s.rise as f64)).exp() as f32;let mut lambda=s.initial;
    if sink.is_some()&&diagonal{p["Segmenter::min_num_pixels"]=Value::from(0);}
    for iteration in 0..s.iterations{publish(sink,3,1,counter(iteration,s.iterations),ProgressUpdate{indices:Some(&optimizer.indices),minimum_pixels:diagonal.then_some(0),..Default::default()})?;optimizer.iterate(lambda as f64,OptimizerMode::Axis,Some(if diagonal{0}else{s.minimum as i64}))?;if (iteration+1)%s.renumber==0{optimizer.rebuild();}lambda*=multiplier;if lambda>s.final_{lambda=s.final_;}}
    if diagonal{optimizer.iterate(lambda as f64,OptimizerMode::Eight,Some(0))?;optimizer.repair_diagonal_contacts();if sink.is_some(){p["Segmenter::min_num_pixels"]=Value::from(s.minimum);}}
    publish(sink,3,2,0,ProgressUpdate{indices:Some(&optimizer.indices),minimum_pixels:diagonal.then_some(s.minimum),..Default::default()})?;optimizer.rebuild();if s.minimum>0{optimizer.iterate(lambda as f64,OptimizerMode::Small,None)?;optimizer.rebuild();}optimizer.publish(sink,3,3,0,None)?;Ok((optimizer,lambda))
}
pub fn presegment_regions(r:Raster<'_>,parameters:&Value)->Result<RegionSnapshot,String>{let s=settings(parameters)?;crate::region_data::read_raster_input(r.width as i64,r.height as i64,r.bgra).map_err(|e|e.0)?;Ok(run_base(r,&s,false,&mut parameters.clone(),&mut None)?.0.snapshot())}
fn finish(mut optimizer:RegionOptimizer,palette:Vec<f32>,lambda:f32,sink:&mut Option<&mut ProgressSink<'_>>)->Result<SegmentResult,String>{
    optimizer.publish(sink,6,-1,0,Some(&palette))?;
    if optimizer.minimum_pixels>0{optimizer.iterate(lambda as f64,OptimizerMode::Small,None)?;optimizer.rebuild();}
    let out=optimizer.snapshot();let result=SegmentResult{indices:out.indices,counts:out.counts,sums:out.sums,color_indices:out.color_indices,palette};
    publish(sink,6,-2,0,ProgressUpdate{indices:Some(&result.indices),counts:Some(&result.counts),palette:Some(&result.palette),color_indices:Some(&result.color_indices),..Default::default()})?;Ok(result)
}
pub(crate) fn validate_palette(palette:&[f32])->Result<(),String>{
    if palette.is_empty()||palette.len()%4!=0{return Err("Palette must contain four normalized BGRA channels per color.".into());}
    if palette.iter().any(|v|!v.is_finite()||!(0.0..=1.0).contains(v)){return Err("Palette channels must be finite numbers from 0 to 1.".into());}
    Ok(())
}
pub fn segment_with_moments(r:Raster<'_>,source:Raster<'_>,parameters:&mut Value,palette:Option<&[f32]>,mut sink:Option<&mut ProgressSink<'_>>)->Result<SegmentResult,String>{
    crate::region_data::read_raster_input(r.width as i64,r.height as i64,r.bgra).map_err(|e|e.0)?;
    let s=settings(parameters)?;
    if let Some(palette)=palette{validate_palette(palette)?;}
    match parameters["Shared::is_anti_aliased"].as_f64(){Some(0.0)|Some(1.0)=>{},_=>return Err("Shared::is_anti_aliased must be 0 or 1.".into())}
    if s.antialias{
        let mut initial=run_base(r,&s,false,parameters,&mut sink)?.0;
        let optimized=crate::subpixel::segment_subpixel_state(r,source,&initial.labels(),parameters,palette,&mut sink)?;
        let (colors,indices)=crate::subpixel::transfer_subpixel_palette(&optimized,palette);
        let optimizer=RegionOptimizer::owned(r,indices,s.final_,s.minimum,Some(colors.clone()));optimizer.publish(&mut sink,4,-2,0,Some(&colors))?;return finish(optimizer,colors,s.final_,&mut sink);
    }
    if let Some(colors)=palette{
        let mut labels=vec![0;r.width*r.height];crate::palette::assign_indices(r.bgra,colors,&mut labels);
        publish(&mut sink,2,0,0,ProgressUpdate{indices:Some(&labels),..Default::default()})?;
        if sink.is_some(){RegionOptimizer::owned(r,labels.clone(),s.final_,s.minimum,Some(colors.to_vec())).publish(&mut sink,2,1,0,None)?;}
        publish(&mut sink,2,2,0,ProgressUpdate{indices:Some(&labels),..Default::default()})?;
        repair_diagonal_map(r.width,r.height,r.bgra,&mut labels);publish(&mut sink,2,3,0,ProgressUpdate{indices:Some(&labels),..Default::default()})?;publish(&mut sink,2,4,0,ProgressUpdate::default())?;
        return finish(RegionOptimizer::owned(r,labels,s.final_,s.minimum,Some(colors.to_vec())),colors.to_vec(),s.final_,&mut sink);
    }
    let (mut optimizer,lambda)=run_base(r,&s,s.diagonal,parameters,&mut sink)?;
    if s.diagonal&&sink.is_some(){for region in (0..optimizer.region_count()).step_by(4){optimizer.publish(&mut sink,3,4,counter(region,optimizer.region_count().saturating_sub(1)),None)?;}}
    let colors=optimizer.project_palette(s.diagonal)?;finish(optimizer,colors,lambda,&mut sink)
}
pub fn segment(r:Raster<'_>,parameters:&Value,palette:Option<&[f32]>)->Result<SegmentResult,String>{segment_with_moments(r,r,&mut parameters.clone(),palette,None)}
pub fn segment_unweighted(r:Raster<'_>,parameters:&Value,palette:Option<&[f32]>)->Result<SegmentResult,String>{if parameters["Shared::is_anti_aliased"].as_f64()!=Some(0.0){return Err("segmentUnweighted requires anti-aliasing disabled.".into());}segment(r,parameters,palette)}
