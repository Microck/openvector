//! Viewer keyboard/layer state and pointer chord producer. Port of
//! `gui/viewer-input.mjs`; contract `docs/viewer-input-contract.json`.

/// Native tool (input mode) identifiers.
pub mod tool {
    pub const PAN: i32 = 0;
    pub const PENCIL: i32 = 3;
    pub const PICKER: i32 = 4;
    pub const FILL: i32 = 5;
    pub const ZAP: i32 = 6;
    pub const BACKGROUND: i32 = 7;
    pub const BATCH: i32 = 8;
    pub const LOCKED: i32 = 10;
}

/// Layer identifiers: Bitmap 0, Segmentation 1, Vector 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Bitmap = 0,
    Segments = 1,
    Vector = 2,
}

impl Layer {
    pub fn from_index(index: i32) -> Option<Layer> {
        match index {
            0 => Some(Layer::Bitmap),
            1 => Some(Layer::Segments),
            2 => Some(Layer::Vector),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Layer::Bitmap => "Bitmap",
            Layer::Segments => "Segments",
            Layer::Vector => "Vector",
        }
    }
}

pub const MODIFIER_SHIFT: u32 = 0x0200_0000;
pub const MODIFIER_CONTROL: u32 = 0x0400_0000;
pub const MODIFIER_ALT: u32 = 0x0800_0000;
pub const MODIFIER_META: u32 = 0x1000_0000;
pub const KEY_SHIFT: i64 = 0x0100_0020;
pub const KEY_B: i64 = 66;
pub const KEY_V: i64 = 86;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPhase {
    Press,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewerKey {
    pub phase: KeyPhase,
    /// Qt key code (nonnegative int32).
    pub key: i64,
    /// Qt keyboard modifier bits (uint32).
    pub modifiers: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputSnapshot {
    pub tool: i32,
    pub primary_layer: i32,
    pub base_layer: i32,
    pub current_layer: i32,
    pub shift_held: bool,
    pub pan_restore: i32,
    pub held_vector: bool,
    pub held_bitmap: bool,
    pub blocked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerAvailability {
    pub bitmap_enabled: bool,
    pub vector_enabled: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidKey;

impl std::fmt::Display for InvalidKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Viewer keys need press/release, a nonnegative int32 key and uint32 modifiers.")
    }
}

/// Live view state, distinct from bitmap edits and region visibility. The
/// reference retains a Pan restore tool and an independently overridden
/// layer; neither derives from a conventional stack of held keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewerInput {
    state: InputSnapshot,
}

impl Default for ViewerInput {
    fn default() -> Self {
        Self {
            state: InputSnapshot {
                tool: tool::PAN,
                primary_layer: 0,
                base_layer: 0,
                current_layer: 0,
                shift_held: false,
                pan_restore: 0,
                held_vector: false,
                held_bitmap: false,
                blocked: false,
            },
        }
    }
}

impl ViewerInput {
    pub fn snapshot(&self) -> InputSnapshot {
        self.state
    }

    /// A key released in another window cannot deliver its release here.
    pub fn release_keys(&mut self) {
        self.state.held_bitmap = false;
        self.state.held_vector = false;
        self.state.current_layer = self.state.base_layer;
        if self.state.shift_held && self.state.tool == tool::PAN {
            self.state.tool = self.state.pan_restore;
        }
        self.state.shift_held = false;
    }

    pub fn set_tool(&mut self, tool: i32) -> InputSnapshot {
        self.state.tool = tool;
        self.state
    }

    /// Selecting the active tool again returns to Pan, except Pan, 1 and 10.
    pub fn select_tool(&mut self, tool: i32) -> InputSnapshot {
        let state = &mut self.state;
        state.tool = if tool == state.tool && ![0, 1, 10].contains(&tool) { 0 } else { tool };
        self.state
    }

    pub fn set_layer(&mut self, layer: i32, segments_enabled: bool, vector_enabled: bool) -> InputSnapshot {
        let base = if (layer == 1 && !segments_enabled) || (layer == 2 && !vector_enabled) { 0 } else { layer };
        self.state.primary_layer = layer;
        self.state.base_layer = base;
        self.state.current_layer = base;
        self.state
    }

    pub fn set_batch_mode(&mut self, blocked: bool) -> InputSnapshot {
        self.state.blocked = blocked;
        self.state.tool = if blocked { tool::BATCH } else { tool::PAN };
        self.state
    }

    pub fn key(&mut self, event: ViewerKey, layers: LayerAvailability) -> Result<InputSnapshot, InvalidKey> {
        if event.key < 0 || event.key > 2147483647 || event.modifiers > 4294967295 {
            return Err(InvalidKey);
        }
        let state = &mut self.state;
        if state.tool == tool::LOCKED || state.blocked {
            return Ok(*state);
        }
        let shift = event.modifiers & MODIFIER_SHIFT as u64 != 0;
        match event.phase {
            KeyPhase::Press => {
                state.shift_held = shift;
                state.held_vector = event.key == KEY_V;
                state.held_bitmap = event.key == KEY_B;
                if state.held_bitmap && layers.bitmap_enabled {
                    state.current_layer = 0;
                }
                if state.held_vector && layers.vector_enabled {
                    state.current_layer = 2;
                }
                if shift && state.tool != tool::PAN {
                    state.pan_restore = state.tool;
                    state.tool = tool::PAN;
                }
            }
            KeyPhase::Release => {
                if event.key == KEY_V {
                    state.held_vector = false;
                }
                if event.key == KEY_B {
                    state.held_bitmap = false;
                }
                if !state.held_bitmap && !state.held_vector {
                    state.current_layer = state.base_layer;
                }
                if !shift {
                    if state.shift_held && state.tool == tool::PAN {
                        state.tool = state.pan_restore;
                    }
                    state.shift_held = false;
                }
            }
        }
        Ok(*state)
    }
}

/// Qt key code for a GPUI key name: Shift, else single characters upper-cased.
pub fn qt_key_code(key: &str) -> i64 {
    if key == "shift" {
        return KEY_SHIFT;
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.to_uppercase().next().map(|u| u as i64).unwrap_or(0),
        _ => 0,
    }
}

pub fn qt_modifiers(shift: bool, control: bool, alt: bool, meta: bool) -> u64 {
    (if shift { MODIFIER_SHIFT } else { 0 }
        | if control { MODIFIER_CONTROL } else { 0 }
        | if alt { MODIFIER_ALT } else { 0 }
        | if meta { MODIFIER_META } else { 0 }) as u64
}

/// Qt-style mouse button bits.
pub mod button {
    pub const LEFT: u32 = 1;
    pub const RIGHT: u32 = 2;
    pub const MIDDLE: u32 = 4;
    pub const BACK: u32 = 8;
    pub const FORWARD: u32 = 16;
    pub const ERASER: u32 = 32;
}

/// DOM button index to Qt bit (left, middle, right, back, forward, eraser).
const DOM_POINTER_BUTTONS: [u32; 6] = [button::LEFT, button::MIDDLE, button::RIGHT, button::BACK, button::FORWARD, button::ERASER];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    Move,
    Press,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewerPointer {
    pub phase: PointerPhase,
    /// Full held-button mask after the event (Qt bits).
    pub buttons: u32,
    /// Changed button (Qt bit), 0 for movement.
    pub button: u32,
    pub modifiers: u64,
}

/// W3C PointerEvent producer: a changed button with its held bit set is a
/// press, a cleared bit a release, button -1 is movement. DOM `buttons` uses
/// the same bit layout (1 left, 2 right, 4 middle).
pub fn dom_pointer(dom_button: i32, dom_buttons: u32, modifiers: u64) -> ViewerPointer {
    let button = usize::try_from(dom_button).ok().and_then(|i| DOM_POINTER_BUTTONS.get(i)).copied().unwrap_or(0);
    let phase = if button == 0 {
        PointerPhase::Move
    } else if dom_buttons & button != 0 {
        PointerPhase::Press
    } else {
        PointerPhase::Release
    };
    ViewerPointer { phase, buttons: dom_buttons, button, modifiers }
}

/// Native pointer-drag policy shared by every producer: tools use the left
/// held bit regardless of which button caused the press; Pan starts on a
/// changed left press and ends on any release (Qt 4.4.3 ScrollHandDrag).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragAction {
    None,
    StartPan,
    EndPan,
    StartPaint,
    EndPaint,
}

pub fn drag_action(pointer: ViewerPointer, editing_tool: bool, dragging: bool) -> DragAction {
    match pointer.phase {
        PointerPhase::Release if dragging => {
            if editing_tool { DragAction::EndPaint } else { DragAction::EndPan }
        }
        PointerPhase::Press if !dragging => {
            if editing_tool {
                if pointer.buttons & button::LEFT != 0 { DragAction::StartPaint } else { DragAction::None }
            } else if pointer.button == button::LEFT {
                DragAction::StartPan
            } else {
                DragAction::None
            }
        }
        _ => DragAction::None,
    }
}
