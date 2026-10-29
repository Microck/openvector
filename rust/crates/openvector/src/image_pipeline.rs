//! Source preparation and automatic classification over the shared engine.
//! The known encoded-PNG alpha-premultiplication mismatch is deliberately not
//! compensated here: native initial alpha preparation remains exactly once.
use crate::{image_resize::{OwnedImage,reduce_working_image},image_preparation::prepare_image_pixels,image_classification::*,region_data::{Raster,read_raster_input},palette::{CoreSession,AnalysisControls,ColorAnalysis,PreparedPaletteAnalysis},segmentation::{segment_with_moments,SegmentResult}};
use serde_json::Value;
#[derive(Clone,Debug)]
pub struct ImageState{pub source:OwnedImage,pub working:OwnedImage}
#[derive(Clone,Debug)]
pub struct ImageAnalysis{pub classification:ImageClassification,pub features:[f32;20],pub regions:SegmentResult,pub blurred:OwnedImage,pub gradient:Vec<f32>,pub blur_gradient:Vec<f32>,pub measurement:Vec<u8>}
#[derive(Clone,Debug)]
pub struct AnalyzedImage{pub image:ImageState,pub parameters:Value,pub image_analysis:ImageAnalysis,pub palette_analysis:ColorAnalysis,pub recommendation_scores:Vec<f32>,pub random_state:u32}
pub fn prepare_original_image(width:usize,height:usize,bgra:&[u8],has_alpha:bool)->Result<OwnedImage,String>{prepare_decoded_source(width,height,bgra,has_alpha,0,0)}
fn prepare_decoded_source(width:usize,height:usize,bgra:&[u8],has_alpha:bool,mode:u32,amount:u32)->Result<OwnedImage,String>{
    read_raster_input(width as i64,height as i64,bgra).map_err(|e|e.to_string())?;
    if mode>3||amount>3{return Err("Image preparation requires explicit mode and amount from 0 through 3.".into())}
    if !has_alpha{
        let mut gray=true;let mut colors=std::collections::HashSet::new();
        for pixel in bgra.chunks_exact(4){gray &= pixel[0]==pixel[1]&&pixel[1]==pixel[2];colors.insert([pixel[0],pixel[1],pixel[2]]);if colors.len()>256{break}}
        if !gray&&colors.len()<=256{let mut pixels=bgra.to_vec();for p in pixels.chunks_exact_mut(4){p[3]=255}return prepare_image_pixels(width,height,&pixels,mode,amount)}
    }
    prepare_image_pixels(width,height,bgra,mode,amount)
}
pub fn prepare_image_state(width:usize,height:usize,bgra:&[u8],has_alpha:bool,mode:u32,amount:u32,parameters:&Value)->Result<ImageState,String>{
    let aa=parameters["Shared::is_anti_aliased"].as_f64();if aa!=Some(0.0)&&aa!=Some(1.0){return Err("Image preparation requires an explicit anti-alias control of 0 or 1.".into())}
    let source=prepare_decoded_source(width,height,bgra,has_alpha,mode,amount)?;let working=reduce_working_image(source.width,source.height,&source.bgra,aa==Some(1.0),true);Ok(ImageState{source,working})
}
pub fn classify_image_state(image:&ImageState,extension:&str,parameters:&Value)->Result<ImageAnalysis,String>{
    let source=read_raster_input(image.source.width as i64,image.source.height as i64,&image.source.bgra).map_err(|e|e.to_string())?;
    let r=Raster{width:image.working.width,height:image.working.height,bgra:&image.working.bgra};
    if r.width<2||r.height<2{return Err("Image classification requires a prepared image of at least 2 by 2.".into())}
    if r.width>source.width||r.height>source.height{return Err("Working image must fit within the preprocessed source.".into())}
    let scale=parameters["EdgeImage::edge_weight_scale"].as_f64().unwrap_or(f64::NAN);let floor=parameters["EdgeImage::edge_weight_floor"].as_f64().unwrap_or(f64::NAN);
    if !scale.is_finite()||scale<0.0||!floor.is_finite()||!(0.0..=1.0).contains(&floor){return Err("Classifier edge controls require a nonnegative scale and a floor from 0 to 1.".into())}
    let regions=segment_with_moments(r,source,&mut parameters.clone(),None,None)?;
    let blurred=OwnedImage{width:source.width,height:source.height,bgra:blur_classifier_pixels(source.width,source.height,source.bgra)};
    let gradients=measure_classifier_gradients(r.width,r.height,r.bgra,&blurred.bgra,blurred.width);
    let mut features=[0.0;20];features[..2].copy_from_slice(&gradients.root_mean_square);features[2..5].copy_from_slice(&measure_classifier_edges(r.width,r.height,r.bgra));
    features[5..14].copy_from_slice(&measure_classifier_regions(r.width,r.height,r.bgra,ClassifierRegions{indices:&regions.indices,counts:&regions.counts,sums:&regions.sums}));
    let colors=measure_classifier_colors(r.width,r.height,r.bgra,scale,floor);features[14..].copy_from_slice(&colors.features);
    Ok(ImageAnalysis{classification:classify_image_features(&features,extension),features,regions,blurred,gradient:gradients.gradient,blur_gradient:gradients.blur_gradient,measurement:colors.measurement})
}
pub fn analyze_prepared_image(session:&CoreSession,image:ImageState,parameters:&Value,extension:&str,mut palette_controls:AnalysisControls)->Result<AnalyzedImage,String>{
    let image_analysis=classify_image_state(&image,extension,parameters)?;let classification=image_analysis.classification;let mut parameters=parameters.clone();
    parameters["Shared::image_type_code"]=((2-classification.image_type)*3+2-classification.quality).into();parameters["Shared::is_anti_aliased"]=i32::from(classification.image_type!=2).into();
    let working=reduce_working_image(image.source.width,image.source.height,&image.source.bgra,classification.image_type!=2,true);
    palette_controls.width=working.width;palette_controls.height=working.height;palette_controls.anti_alias=classification.image_type!=2;
    let palette_analysis=PreparedPaletteAnalysis::from_retained_measurement(&working.bgra,palette_controls,&image_analysis.measurement,image.working.width)?.analyze(session,&working.bgra)?;
    let recommendation_scores=palette_analysis.candidates.iter().map(|candidate|if palette_analysis.classification.mode==2{candidate.score.total}else{if candidate.color_count!=palette_analysis.classification.color_count as usize{1.0}else{0.0}}).collect();
    Ok(AnalyzedImage{image:ImageState{source:image.source,working},parameters,image_analysis,palette_analysis,recommendation_scores,random_state:session.random_state()})
}

/// JSON boundary used by the CLI and language bindings; all computation stays
/// in the same typed pipeline used by documents.
pub fn analyze_image(session:&CoreSession,input:&Value)->Result<AnalyzedImage,String>{
    if !input.is_object(){return Err("Expected an image analysis object.".into())}
    let parameters=&input["parameters"];let palette=&input["paletteControls"];
    if !palette.is_object(){return Err("Image analysis requires explicit paletteControls.".into())}
    if !parameters["shadowing"].is_boolean()||!parameters["clusterColors"].is_boolean(){return Err("Image analysis requires explicit shadowing and clusterColors switches.".into())}
    let dimension=|name:&str|input[name].as_u64().filter(|v|*v<=i32::MAX as u64).map(|v|v as usize).ok_or_else(||"Region dimensions must be positive int32 values with an int32 pixel count.".to_string());
    let width=dimension("width")?;let height=dimension("height")?;
    let pixels=input["bgra"].as_array().ok_or("Expected BGRA byte pixels.")?;
    let bgra=pixels.iter().map(|v|v.as_u64().filter(|v|*v<=255).map(|v|v as u8).ok_or("Expected BGRA byte pixels.".to_string())).collect::<Result<Vec<_>,_>>()?;
    let has_alpha=input["hasAlpha"].as_bool().ok_or("Decoded image preparation requires an explicit hasAlpha boolean.")?;
    let mode=input["mode"].as_u64().filter(|v|*v<=3).ok_or("Image preparation requires explicit mode and amount from 0 through 3.")? as u32;
    let amount=input["amount"].as_u64().filter(|v|*v<=3).ok_or("Image preparation requires explicit mode and amount from 0 through 3.")? as u32;
    let extension=input["extension"].as_str().ok_or("Image classification requires an explicit extension string.")?;
    let image=prepare_image_state(width,height,&bgra,has_alpha,mode,amount,parameters)?;
    let controls=AnalysisControls{width,height,bins:palette["bins"].as_u64().unwrap_or(0) as usize,connected:palette["connected"].as_bool().ok_or("connected must be boolean.")?,anti_alias:false,cost_per_color:palette["costPerColor"].as_f64().unwrap_or(f64::NAN),edge_scale:parameters["EdgeImage::edge_weight_scale"].as_f64().unwrap_or(f64::NAN),edge_floor:parameters["EdgeImage::edge_weight_floor"].as_f64().unwrap_or(f64::NAN)};
    analyze_prepared_image(session,image,parameters,extension,controls)
}
impl AnalyzedImage{
    pub fn to_json(&self)->Value{
        let c=self.image_analysis.classification;let p=self.palette_analysis.classification;
        serde_json::json!({"imageClassification":{"label":c.label,"imageType":c.image_type,"quality":c.quality},"imageTypeCode":self.parameters["Shared::image_type_code"],"antiAlias":self.parameters["Shared::is_anti_aliased"].as_f64()!=Some(0.0),"width":self.image.working.width,"height":self.image.working.height,"paletteClassification":{"label":p.label,"mode":p.mode,"colorCount":p.color_count},"candidates":self.palette_analysis.candidates.iter().zip(&self.recommendation_scores).map(|(c,score)|serde_json::json!({"colorCount":c.color_count,"palette":c.generated.palette,"weights":c.generated.weights,"error":c.score.error,"colorCost":c.score.colors,"recommendationScore":score})).collect::<Vec<_>>(),"randomState":self.random_state})
    }
}
