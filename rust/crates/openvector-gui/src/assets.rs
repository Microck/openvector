use std::borrow::Cow;
use gpui::{App, AssetSource, Result, SharedString};

pub struct Assets;
const ICONS: [(&str, &[u8]); 15] = [
    ("arrow-up", include_bytes!("../assets/icons/arrow-up.svg")),
    ("arrow-down", include_bytes!("../assets/icons/arrow-down.svg")),
    ("arrow-left", include_bytes!("../assets/icons/arrow-left.svg")),
    ("arrow-right", include_bytes!("../assets/icons/arrow-right.svg")),
    ("zoom-in", include_bytes!("../assets/icons/zoom-in.svg")),
    ("zoom-out", include_bytes!("../assets/icons/zoom-out.svg")),
    ("folder-open", include_bytes!("../assets/icons/folder-open.svg")),
    ("clipboard", include_bytes!("../assets/icons/clipboard.svg")),
    ("layers", include_bytes!("../assets/icons/layers.svg")),
    ("export", include_bytes!("../assets/icons/export.svg")),
    ("sparkles", include_bytes!("../assets/icons/sparkles.svg")),
    ("sliders", include_bytes!("../assets/icons/sliders.svg")),
    ("image", include_bytes!("../assets/icons/image.svg")),
    ("pen", include_bytes!("../assets/icons/pen.svg")),
    // Monochrome mark; svg() tints it with the current text colour.
    ("logo", include_bytes!("../assets/logo.svg")),
];
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> { Ok(ICONS.iter().find(|(name, _)| *name == path).map(|(_, bytes)| Cow::Borrowed(*bytes))) }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> { Ok(ICONS.iter().filter(|(name, _)| name.starts_with(path)).map(|(name, _)| (*name).into()).collect()) }
}
pub fn register_fonts(cx: &App) -> Result<()> {
    cx.text_system().add_fonts(vec![Cow::Borrowed(include_bytes!("../assets/fonts/Geist-Regular.ttf")), Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono-Regular.ttf"))])
}
