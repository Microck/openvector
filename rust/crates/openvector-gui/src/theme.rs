// Charcoal surfaces with one blue accent. The app has a single dark theme;
// there is no light variant.
pub const BACKGROUND: u32 = 0x17181b;
/// Header, tool rail, wizard and status bar panels.
pub const SURFACE: u32 = 0x1e1f23;
/// Canvas behind the artwork, a step darker than the panels.
pub const VIEWPORT: u32 = 0x121316;
pub const TEXT: u32 = 0xe4e5e7;
pub const MUTED: u32 = 0x8b8d93;
/// Dialogs, tooltips and other raised overlays.
pub const RAISED: u32 = 0x26272c;
/// Resting fill of buttons. Kept a step above RAISED so buttons stay visible
/// inside dialogs, which sit on RAISED.
pub const CONTROL: u32 = 0x313238;
pub const BORDER: u32 = 0x2a2b30;
/// Button outline, lighter than BORDER for the same reason as CONTROL.
pub const CONTROL_BORDER: u32 = 0x3d3e45;
pub const HOVER: u32 = 0x3a3b42;
/// Selected options: a blue tint so the active choice stays visible.
pub const SELECTED: u32 = 0x24324f;
pub const ACCENT: u32 = 0x2f6fec;
pub const DANGER: u32 = 0xf87171;
pub const SUCCESS: u32 = 0x34d399;
pub const HEADER_HEIGHT: f32 = 38.0;
// Include clearance for the processing Cancel control and the footer border.
pub const FOOTER_HEIGHT: f32 = CONTROL_HEIGHT + 4.0;
pub const CONTROL_HEIGHT: f32 = 28.0;
pub const CONTROL_RADIUS: f32 = 6.0;
pub const DIALOG_RADIUS: f32 = 10.0;
pub const WIZARD_WIDTH: f32 = 320.0;
pub const UI_FONT_SIZE: f32 = 13.0;
