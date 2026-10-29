use crate::{Result,read_json,print_json};
use openvector::{palette::{CoreSession,AnalysisControls},contours::{Geometry,ContourInput},contour_fitting::{FittedGeometry,FittingParameters},vector_output::VectorOutputInput};
use serde_json::{json,Value};
use std::io::Write;
fn integer(v:&Value,key:&str)->Result<i64>{v[key].as_i64().ok_or_else(||format!("{key} must be an integer."))}
fn array<T:serde::de::DeserializeOwned>(v:&Value,key:&str)->Result<Vec<T>>{serde_json::from_value(v[key].clone()).map_err(|e|format!("Invalid {key}: {e}"))}
pub fn export(format:&str,geometry:&FittedGeometry,document:&Value)->Result<Vec<u8>>{
    let label=format.to_uppercase();
    if document.get("onProgress").is_some(){return Err(format!("{label} onProgress must be a function."))}
    let dimension=|key:&str|document[key].as_i64().filter(|v|*v>0&&*v<=i32::MAX as i64).ok_or_else(||format!("{label} needs positive int32 source dimensions and both output dimensions or neither."));
    let width=dimension("width")?;let height=dimension("height")?;
    let hidden=if document.get("hiddenRegions").is_some(){array::<i64>(document,"hiddenRegions")?}else{Vec::new()};
    let mut input=VectorOutputInput::new(geometry,width,height);input.hidden_regions=&hidden;
    if document.get("outputWidth").is_some()||document.get("outputHeight").is_some(){input.output_size=Some((dimension("outputWidth")?,dimension("outputHeight")?));}
    let invalid_layout=||format!("{label} layout must be stacked, cutouts or grouped-cutouts; thinStroke must be true or false.");
    if let Some(layout)=document.get("layout"){input.layout=layout.as_str().and_then(openvector::export_options::Layout::parse).ok_or_else(invalid_layout)?;}
    if let Some(thin)=document.get("thinStroke"){input.thin_stroke=thin.as_bool().ok_or_else(invalid_layout)?;}
    let result=match format{
        "svg"=>openvector::svg::export_svg(&input).map(String::into_bytes),
        "eps"=>openvector::eps::export_eps(&input,&openvector::eps::EpsOptions{compact:document.get("compact").map(|v|v.as_bool().ok_or("EPS compact must be true or false; creationDate must be 1..255 printable ASCII characters on one line.")).transpose()?.unwrap_or(false),creation_date:document.get("creationDate").map(|v|v.as_str().map(str::to_owned).ok_or("EPS compact must be true or false; creationDate must be 1..255 printable ASCII characters on one line.")).transpose()?}).map(String::into_bytes),
        "ai"=>openvector::ai::export_ai(&input).map(String::into_bytes),
        "pdf"=>openvector::pdf::export_pdf(&input),
        "dxf"=>{
            let palette=array::<f32>(document,"palette").map_err(|error|format!("DXF: {error}"))?;
            let mode=match document.get("curveMode").map(Value::as_str).unwrap_or(Some("dense-lines")){ Some("splines")=>openvector::dxf::DxfCurveMode::Splines,Some("sparse-lines")=>openvector::dxf::DxfCurveMode::SparseLines,Some("dense-lines")=>openvector::dxf::DxfCurveMode::DenseLines,_=>return Err("DXF curveMode must be splines, sparse-lines or dense-lines.".into())};
            openvector::dxf::export_dxf(&input,&palette,mode).map(String::into_bytes)
        },
        "emf"=>{
            let device=&document["referenceDevice"];
            let pair=|key:&str|->Result<[i32;2]>{serde_json::from_value(device[key].clone()).map_err(|_|"EMF referenceDevice needs pixels, millimeters and micrometers pairs of positive int32 integers.".into())};
            openvector::emf::export_emf(&input,&openvector::emf::EmfDevice{pixels:pair("pixels")?,millimeters:pair("millimeters")?,micrometers:pair("micrometers")?})
        },
        _=>return Err(format!("Unknown export format: {format}")),
    };result.map_err(|e|e.to_string())
}
pub fn run(command:&str,args:&[String])->Result<()>{
    if args.len()!=if command=="fill"{3}else{1} || !["settings","fill","assign-palette","refine-palette","generate-palette","analyze-image","segment","trace-contours","smooth-contours","fit-contours","export-svg","export-eps","export-pdf","export-ai","export-dxf","export-emf"].contains(&command){return Err("Expected settings, fill, assign-palette, refine-palette, generate-palette, analyze-image, segment, trace-contours, smooth-contours, fit-contours, export-svg, export-eps, export-pdf, export-ai, export-dxf, or export-emf with JSON input. Use --help for syntax.".into())}
    let document=read_json(&args[0])?;
    if let Some(format)=command.strip_prefix("export-"){let geometry=FittedGeometry::from_json(&document["geometry"]).map_err(|error|format!("{}: {error}",format.to_uppercase()))?;let bytes=export(format,&geometry,&document)?;return std::io::stdout().write_all(&bytes).map_err(|e|e.to_string())}
    let session=CoreSession::default();
    let output=match command{
        "settings"=>openvector::settings::resolve_parameters(&document)?,
        "fill"=>{
            let parse=|value:&str,name:&str|->Result<i64>{if value.is_empty()||!value.strip_prefix('-').unwrap_or(value).bytes().all(|b|b.is_ascii_digit()){return Err(format!("{name} must be an integer."));}value.parse().map_err(|_|format!("{name} must be an integer."))};
            let mut map=openvector::pixel_map::PixelMap::new(integer(&document,"width")?,integer(&document,"height")?,&array::<i64>(&document,"segments")?).map_err(|e|e.to_string())?;
            map.flood_fill(parse(&args[1],"seed")?,parse(&args[2],"replacement")?).map_err(|e|e.to_string())?;
            json!({"width":map.width(),"height":map.height(),"segments":map.segments()})
        },
        "assign-palette"=>json!(openvector::palette::assign_palette(&array::<u8>(&document,"bgra")?,&array::<f64>(&document,"palette")?).map_err(|e|e.to_string())?),
        "generate-palette"|"refine-palette"=>{
            let aa=document["antiAlias"].as_bool().ok_or("Palette commands require antiAlias: true or false.")?;
            let bgra=array::<u8>(&document,"bgra")?;
            let generated=if command=="refine-palette"{let colors=array::<f64>(&document,"palette")?;if aa{session.refine_weighted(integer(&document,"width")?,integer(&document,"height")?,&bgra,&colors)?}else{session.refine_unweighted(&bgra,&colors)?}}
            else{let count=usize::try_from(integer(&document,"colorCount")?).map_err(|_|"colorCount must be a positive int32 integer.")?;let bins=usize::try_from(integer(&document,"bins")?).map_err(|_|"Histogram bins must be 8 or 16.")?;let connected=document["connected"].as_bool().ok_or("connected must be boolean.")?;
                if aa{session.generate_weighted(&bgra,count,AnalysisControls{width:usize::try_from(integer(&document,"width")?).map_err(|_|"Invalid width.")?,height:usize::try_from(integer(&document,"height")?).map_err(|_|"Invalid height.")?,bins,connected,anti_alias:true,cost_per_color:0.0,edge_scale:50.0,edge_floor:0.01})?}else{session.generate_unweighted(&bgra,count,bins,connected)?}};
            json!({"palette":generated.palette,"indices":generated.indices,"weights":generated.weights,"passes":generated.passes})
        },
        "fit-contours"=>{
            let parameters=parameters(&document,"fitting","Fitting")?;
            let geometry=Geometry::from_json(&document["geometry"]).ok_or("Fitting needs a completed smoothed graph.")?;
            openvector::contour_fitting::fit_contours(&geometry,&FittingParameters::from_json(&parameters),None)?.to_json()
        },
        "smooth-contours"=>{
            let parameters=parameters(&document,"smoothing","Smoothing")?;
            let geometry=Geometry::from_json(&document["geometry"]).ok_or("Smoothing needs shared contour geometry.")?;
            let bgra=array::<u8>(&document,"bgra")?;let indices=array::<i64>(&document,"indices")?;let palette=array::<f64>(&document,"palette")?;
            let smoothed=session.smooth_contours(&openvector::contour_smoothing::SmoothContourInput{width:integer(&document,"width")?,height:integer(&document,"height")?,bgra:&bgra,geometry:&geometry,indices:&indices,palette:&palette,parameters:&parameters})?;
            json!({"geometry":smoothed.geometry.to_json(),"penaltyCounts":smoothed.penalty_counts})
        },
        "segment"|"trace-contours"=>{
            let mut parameters=parameters(&document,"segmentation","Segmentation")?;if parameters["shadowing"]==true{parameters["shadowControl"]=document["shadowControl"].clone();}
            let bgra=array::<u8>(&document,"bgra")?;let width=integer(&document,"width")?;let height=integer(&document,"height")?;
            let raster=openvector::region_data::read_raster_input(width,height,&bgra).map_err(|e|e.to_string())?;
            let palette=document.get("palette").map(|_|array::<f32>(&document,"palette")).transpose()?;
            let segmented=openvector::segmentation::segment(raster,&parameters,palette.as_deref())?;
            if command=="segment"{json!({"indices":segmented.indices,"counts":segmented.counts,"palette":segmented.palette,"colorIndices":segmented.color_indices})}
            else{let indices=segmented.indices.iter().map(|v|*v as i64).collect::<Vec<_>>();let counts=segmented.counts.iter().map(|v|*v as i64).collect::<Vec<_>>();let colors=segmented.color_indices.iter().map(|v|*v as i64).collect::<Vec<_>>();let palette=segmented.palette.iter().map(|v|*v as f64).collect::<Vec<_>>();
                openvector::contours::extract_contours(&ContourInput{width:width as f64,height:height as f64,indices:&indices,counts:&counts,color_indices:&colors,palette:&palette,anti_alias:Some(parameters["Shared::is_anti_aliased"]==1)}).map_err(|e|e.to_string())?.to_json()}
        },
        "analyze-image"=>openvector::image_pipeline::analyze_image(&session,&document)?.to_json(),
        _=>unreachable!(),
    };print_json(&output)
}
fn parameters(document:&Value,stage:&str,label:&str)->Result<Value>{
    if !document["settings"].is_object(){return Err(format!("{label} needs a settings object with explicit controls."));}
    let mut settings=document["settings"].clone();settings["stage"]=json!(stage);openvector::settings::resolve_parameters(&settings)
}
