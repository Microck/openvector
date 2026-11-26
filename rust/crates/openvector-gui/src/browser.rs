// The platform supplies files and downloads. GPUI owns every screen and interaction.
use gpui::{App, AppContext, ApplicationHandle, Focusable, WindowHandle, WindowOptions};
use std::{cell::RefCell, path::PathBuf};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = openVectorHost, js_name = pickImages)]
    async fn pick_images_js(multiple: bool) -> Result<js_sys::Array, JsValue>;
    #[wasm_bindgen(catch, js_namespace = openVectorHost, js_name = download)]
    fn download_file(path: &str) -> Result<(), JsValue>;
}

pub async fn pick_images(multiple: bool) -> Result<Result<Option<Vec<PathBuf>>, String>, String> {
    let names = pick_images_js(multiple).await.map_err(|error| format!("{error:?}"))?;
    let paths: Vec<_> = names.iter().filter_map(|v| v.as_string().map(PathBuf::from)).collect();
    Ok(Ok((!paths.is_empty()).then_some(paths)))
}

pub fn save_path(name: &str) -> std::future::Ready<Result<Result<Option<PathBuf>, String>, String>> {
    std::future::ready(Ok(Ok(Some(PathBuf::from("/downloads").join(name)))))
}

thread_local! {
    static WINDOW: RefCell<Option<WindowHandle<crate::OpenVectorApp>>> = const { RefCell::new(None) };
    static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub fn start_openvector() {
    gpui_platform::web_init();
    // GPUI paints on the browser thread. Only the document worker needs a pthread.
    let platform = std::rc::Rc::new(gpui_web::WebPlatform::new_with_backend(false, gpui_web::WebBackendPreference::WebGl));
    let http = std::sync::Arc::new(platform.fetch_http_client());
    let app = gpui::Application::with_platform(platform).with_http_client(http).with_assets(crate::assets::Assets);
    let handle = app.run_embedded(|cx: &mut App| {
        crate::assets::register_fonts(cx).expect("Cannot load the OpenVector fonts");
        let window = cx.open_window(WindowOptions::default(), |_, cx| cx.new(|cx| {
            crate::OpenVectorApp::with_preferences("/settings/gui.json".into(), cx).expect("Cannot start OpenVector")
        })).expect("Cannot open OpenVector window");
        WINDOW.with(|slot| *slot.borrow_mut() = Some(window));
        window.update(cx, |app, window, cx| {
            app.focus_handle(cx).focus(window, cx);
        }).expect("Cannot focus OpenVector");
        cx.activate(true);
    });
    APPLICATION.with(|slot| *slot.borrow_mut() = Some(handle));
}

pub fn download(path: &std::path::Path) -> Result<(), String> {
    download_file(&path.to_string_lossy()).map_err(|error| format!("{error:?}"))
}

#[wasm_bindgen]
pub fn open_images(paths: js_sys::Array) {
    let paths = paths.iter().filter_map(|p| p.as_string()).collect::<Vec<_>>();
    APPLICATION.with_borrow(|app| {
        if let Some(app) = app { app.update(|cx| WINDOW.with_borrow(|window| {
            if let Some(window) = window {
                let _ = window.update(cx, |app, _, cx| app.open_browser_files(paths, cx));
            }
        })); }
    });
}
