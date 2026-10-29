#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;
use gpui::{prelude::*, App, Bounds, Focusable, WindowBounds, WindowOptions, px, size};
use openvector_gui::OpenVectorApp;

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let first = arguments.next();
    let smoke = first.as_deref() == Some(std::ffi::OsStr::new("--smoke"));
    let input = if smoke { arguments.next() } else { first };
    if smoke && input.is_none() {
        eprintln!("Usage: openvector-gui --smoke IMAGE [OUTPUT.svg]");
        std::process::exit(2);
    }
    let output = if smoke {
        Some(arguments.next().map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join(format!("openvector-smoke-{}.svg", std::process::id()))))
    } else { None };
    if arguments.next().is_some() {
        eprintln!("Usage: openvector-gui [IMAGE] | --smoke IMAGE [OUTPUT.svg]");
        std::process::exit(2);
    }
    gpui_platform::application().with_assets(openvector_gui::assets::Assets).run(move |cx: &mut App| {
        if let Err(error) = openvector_gui::assets::register_fonts(cx) {
            eprintln!("Cannot load bundled fonts: {error}"); std::process::exit(1);
        }
        let bounds = Bounds::centered(None, size(px(1180.0), px(760.0)), cx);
        let window = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(760.0), px(500.0))),
            ..Default::default()
        }, |window, cx| {
            window.set_window_title("OpenVector");
            let app = cx.new(|cx| {
                let mut app = match OpenVectorApp::new(cx) {
                    Ok(app) => app,
                    Err(error) => { eprintln!("Cannot start OpenVector: {error}"); std::process::exit(1); },
                };
                if let Some(input) = input {
                    if let Some(output) = output { app.start_smoke(input.into(), output, cx); }
                    else { app.open(input.into(), cx); }
                }
                app
            });
            app.focus_handle(cx).focus(window, cx);
            app
        });
        if let Err(error) = window {
            eprintln!("Cannot open OpenVector window: {error}");
            std::process::exit(1);
        }
        cx.activate(true);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() { cx.quit(); }
        }).detach();
    });
}
