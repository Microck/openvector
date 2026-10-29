export const viewerTools = {pan:0,pencil:3,picker:4,fill:5,zap:6,background:7};
export const viewerLayers = ['bitmap','segments','vector'];
const pointerButtons=[1,4,2,8,16,32];

export function viewerModifiers(event) {
  return (event.shiftKey ? 0x02000000 : 0) | (event.ctrlKey ? 0x04000000 : 0) |
    (event.altKey ? 0x08000000 : 0) | (event.metaKey ? 0x10000000 : 0);
}

/** Live view state, distinct from bitmap edits and region visibility. The
 * reference retains a Pan restore tool and an independently overridden layer;
 * neither can be derived from a conventional stack of held browser keys. */
export class ViewerInput {
  #state = {tool:0,primaryLayer:0,baseLayer:0,currentLayer:0,
    shiftHeld:false,panRestore:0,heldVector:false,heldBitmap:false,blocked:false};

  snapshot() { return {...this.#state}; }

  setTool(tool) { this.#state.tool = tool;return this.snapshot(); }

  selectTool(tool) {
    const state = this.#state;
    state.tool = tool === state.tool && ![0,1,10].includes(tool) ? 0 : tool;
    return this.snapshot();
  }

  setLayer(layer,{segmentsEnabled = true,vectorEnabled = true} = {}) {
    const base = layer === 1 && !segmentsEnabled || layer === 2 && !vectorEnabled ? 0 : layer;
    Object.assign(this.#state,{primaryLayer:layer,baseLayer:base,currentLayer:base});
    return this.snapshot();
  }

  setBatchMode(blocked) {
    this.#state.blocked = blocked;this.#state.tool = blocked ? 8 : 0;
    return this.snapshot();
  }

  key({phase,key,modifiers}, {bitmapEnabled,vectorEnabled}) {
    if (!['press','release'].includes(phase) || !Number.isInteger(key) || key < 0 || key > 2147483647 ||
        !Number.isInteger(modifiers) || modifiers < 0 || modifiers > 4294967295)
      throw new Error('Viewer keys need press/release, a nonnegative int32 key and uint32 modifiers.');
    const state = this.#state;
    if (state.tool === 10 || state.blocked) return this.snapshot();
    const shift = Boolean(modifiers&0x02000000);
    if (phase === 'press') {
      state.shiftHeld = shift;
      state.heldVector = key === 86;state.heldBitmap = key === 66;
      if (state.heldBitmap && bitmapEnabled) state.currentLayer = 0;
      if (state.heldVector && vectorEnabled) state.currentLayer = 2;
      if (shift && state.tool !== 0) {state.panRestore = state.tool;state.tool = 0;}
    } else {
      if (key === 86) state.heldVector = false;
      if (key === 66) state.heldBitmap = false;
      if (!state.heldBitmap && !state.heldVector) state.currentLayer = state.baseLayer;
      if (!shift) {
        if (state.shiftHeld && state.tool === 0) state.tool = state.panRestore;
        state.shiftHeld = false;
      }
    }
    return this.snapshot();
  }
}

/** DOM events already expose modifiers after the key transition. Qt's public
 * constructor stores pre-transition bits for modifier keys; only the native
 * recorder needs that conversion. Browser input uses current bits directly. */
export function browserViewerKey(event,phase) {
  return {phase,key:event.key === 'Shift' ? 0x01000020 : event.key.length === 1 ? event.key.toUpperCase().charCodeAt(0) : 0,
    modifiers:viewerModifiers(event)};
}

/** Intermediate button presses/releases arrive as pointermove. Its changed
 * button and the full held mask preserve the native mouse-event phases. */
export function browserViewerPointer(event) {
  const button=pointerButtons[event.button] ?? 0;
  return {phase:button===0?'move':event.buttons&button?'press':'release',
    buttons:event.buttons,button,modifiers:viewerModifiers(event)};
}
