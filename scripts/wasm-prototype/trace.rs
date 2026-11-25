use openvector::{
    export_options::ExportOptions, image_document::ImageDocument, image_input,
    palette::CoreSession, svg::export_svg, vector_output::VectorOutputInput,
};
use serde_json::json;
use std::fs;

fn open(
    bytes: &[u8],
    filename: &str,
    shrink: bool,
) -> Result<
    (
        ImageDocument,
        serde_json::Value,
        openvector::pixel_input::DecodedImage,
    ),
    String,
> {
    let meta = image_input::resolve_image_filename(filename);
    let mut image = image_input::decode_image(bytes).map_err(|e| e.to_string())?;
    let (width, height) = (image.width, image.height);
    let mut document = ImageDocument::new(CoreSession::default());
    document.set_source(
        width,
        height,
        &image.bgra,
        image.has_alpha,
        meta.shadow_control as f64,
    )?;
    if shrink && document.shrink_source() {
        let original = &document.state().original;
        image.width = original.width;
        image.height = original.height;
        image.bgra.clone_from(&original.bgra);
        // The reduced source already contains premultiplied pixels. Canvas needs straight RGBA.
        for pixel in image.bgra.chunks_exact_mut(4) {
            let alpha = u32::from(pixel[3]);
            if alpha != 0 {
                for channel in &mut pixel[..3] {
                    *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
                }
            }
        }
    }
    let trace = document.run_automatic(&json!({"extension": meta.extension}))?;
    Ok((
        document,
        serde_json::Value::Object(trace.settings.to_json()),
        image,
    ))
}

fn render(document: &ImageDocument, options: ExportOptions) -> Result<String, String> {
    let source = &document.state().source;
    let (width, height) = (source.width, source.height);
    let geometry = document
        .presentation_geometry()
        .ok_or("Missing fitted geometry")?;
    let hidden: Vec<i64> = document
        .background_state()
        .hidden_regions
        .iter()
        .map(|&id| id as i64)
        .collect();
    let mut input = VectorOutputInput::new(&geometry, width as i64, height as i64);
    input.layout = options.layout;
    input.thin_stroke = options.thin_stroke;
    input.hidden_regions = &hidden;
    export_svg(&input).map_err(|e| e.to_string())
}

// Emscripten's in-memory filesystem keeps user images inside the worker.
// Both targets time the same Rust call, excluding file transfer and display.
fn run(input: &str, output: &str) -> Result<f64, String> {
    let bytes = fs::read(input).map_err(|e| e.to_string())?;
    let start = std::time::Instant::now();
    let (document, _, _) = open(&bytes, input, false)?;
    let svg = render(&document, ExportOptions::default())?;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    fs::write(output, svg).map_err(|e| e.to_string())?;
    Ok(ms)
}

#[cfg(target_os = "emscripten")]
#[unsafe(no_mangle)]
pub extern "C" fn convert_png() -> f64 {
    run("input.png", "output.svg").unwrap_or_else(|e| {
        fs::write("error.txt", e).unwrap();
        -1.0
    })
}

#[cfg(target_os = "emscripten")]
fn main() {}

#[cfg(not(target_os = "emscripten"))]
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    for _ in 0..args[3].parse::<usize>().map_err(|e| e.to_string())? {
        println!("{}", run(&args[1], &args[2])?);
    }
    Ok(())
}

mod editor;
