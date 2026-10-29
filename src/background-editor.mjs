import {readEditorGesture,readEditorPoint} from './editor-input.mjs';
import {findOutputRegion} from './vector-output.mjs';

const tools = ['pan','background'];

/** One wire grammar for CLI plans and GUI requests. Document methods remain
 * the state owners; this boundary validates commands and ordered batches. */
export function applyBackgroundCommand(document,command) {
  if (!command || typeof command !== 'object' || Array.isArray(command)) throw new Error('Expected a background editing command.');
  switch (command.action) {
    case 'begin': return document.beginBackgroundEditing();
    case 'toggle': return document.toggleBackground(command.point);
    case 'undo': return document.undoBackground();
    case 'redo': return document.redoBackground();
    case 'revert': return document.revertBackground();
    case 'finish': return document.finishBackgroundEditing();
    case 'gesture': {
      if (!Array.isArray(command.events) || !command.events.length) throw new Error('Submit a nonempty array of background gesture events.');
      let background;
      for (const event of command.events) background = document.backgroundGesture(event);
      return background;
    }
    default: throw new Error('Choose begin, gesture, toggle, undo, redo, revert or finish for background editing.');
  }
}

/** Logical visibility outlives individual vector results. Region IDs retained
 * in history can exceed a replacement graph's count; those IDs have no hidden
 * bit in the current logical range, but still move through undo/redo. */
export class BackgroundEditor {
  #hidden = new Uint8Array();
  #undo = [];#redo = [];
  #active = false;#tool = 'pan';

  snapshot() {
    const hiddenRegions = [];
    for (let region = 0; region < this.#hidden.length; region++) if (this.#hidden[region]) hiddenRegions.push(region);
    return {count:this.#hidden.length,active:this.#active,tool:this.#tool,hiddenRegions,
      undo:this.#undo.slice(),redo:this.#redo.slice()};
  }

  begin() {
    if (!this.#hidden.length) throw new Error('Background editing needs a completed vector result.');
    this.#active = true;this.#tool = 'background';return this.snapshot();
  }
  finish() { this.#active = false;this.#tool = 'pan';return this.snapshot(); }

  #clear(count) {
    this.#hidden = new Uint8Array(count);
    this.#redo = this.#undo;this.#undo = [];
  }

  acceptVectors(count) { this.#clear(count); }

  #toggle(region) {
    // Native visibility ignores IDs beyond its logical bit count. Such an ID
    // may remain in history after region replacement and a later redo.
    if (region < this.#hidden.length) this.#hidden[region] ^= 1;
  }

  #applyPoint(point,segmentation,geometry) {
    if (geometry === null) throw new Error('Background hit testing needs completed contour boundaries. Update the vector first.');
    const region = findOutputRegion(segmentation,geometry,point);
    if (region !== -1) { this.#toggle(region);this.#undo.push(region);this.#redo = []; }
    return this.snapshot();
  }

  toggle(point,segmentation,geometry) { return this.#applyPoint(readEditorPoint(point),segmentation,geometry); }

  gesture(input,segmentation,geometry,pointer) {
    const event = readEditorGesture(input,tools);
    if (event.phase === 'select') this.#tool = event.tool === this.#tool ? 'pan' : event.tool;
    else if (event.phase === 'color') throw new Error('Background editing does not change the paint color.');
    else if (this.#tool === 'background' && event.phase === 'press' && (event.buttons&1)) {
      // MainView stores the point before dispatch, including failed hit tests.
      pointer.previousPoint = event.point;
      return this.#applyPoint(event.point,segmentation,geometry);
    }
    return this.snapshot();
  }

  undo() {
    if (this.#undo.length) {
      const region = this.#undo.pop();this.#toggle(region);this.#redo.unshift(region);
    }
    return this.snapshot();
  }

  redo() {
    if (this.#redo.length) {
      const region = this.#redo.shift();this.#toggle(region);this.#undo.push(region);
    }
    return this.snapshot();
  }

  revert() { this.#clear(this.#hidden.length);return this.snapshot(); }
}
