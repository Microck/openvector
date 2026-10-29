import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {CoreSession} from '../src/settings.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {findOutputRegion} from '../src/vector-output.mjs';
import {cleanStartupProfile,compareDocumentState,settingsFromBits} from './helpers/native-gui.mjs';

const [metadata,...workflows] = gunzipSync(readFileSync(new URL('./fixtures/background-edit-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

const tools = {0:'pan',3:'pencil',4:'picker',5:'fill',6:'zap',7:'background'};
const properties = {'setUseAdvancedMode(bool)':['advanced',Boolean],
  'setAdvContourSmoothness(int)':['smoothness',Number],'setAdvCurveComplexity(int)':['curveComplexity',Number]};

function compareFrame(document,selected,page,expected,label) {
  const background = document.backgroundState(),native = expected.visibility;
  assert.equal(background.count,native.header[3],label+' visibility size');
  assert.equal(background.active,Boolean(native.header[5]),label+' mode');
  assert.deepEqual(background.hiddenRegions,native.visible.flatMap((visible,index) => visible ? [] : [index]),label+' hidden regions');
  for (const name of ['undo','redo']) assert.deepEqual(background[name],native[name],label+' background '+name);
  const editor = document.editingState();
  assert.deepEqual(Buffer.from(editor.bgra),Buffer.from(expected.bgra,'hex'),label+' editor pixels');
  assert.equal(editor.key,expected.key,label+' editor key');
  for (const name of ['undo','redo']) assert.deepEqual(editor[name],expected[name].map(({bits,...record}) => record),label+' segmentation '+name);
  assert.equal(background.active ? background.tool : editor.tool,tools[expected.inputMode],label+' tool');
  assert.deepEqual(Array.from(editor.color),expected.paintArgb.slice().reverse(),label+' paint');
  assert.deepEqual(selected,settingsFromBits(expected.selectedSettingsBits,expected.paletteArgbBits),label+' selected controls');
  const pageId = {background:15,editing:14,smoothing:16,export:selected.advanced ? 17 : 19}[page];
  assert.equal(pageId,native.header[4],label+' page');
  assert.equal(page === 'editing' ? 1 : 2,native.header[6],label+' default layer');
}

for (const workflow of workflows) test('native background document session: '+workflow.id,() => {
  // Only own encoded bytes, declared controls and command inputs enter the
  // implementation. Native intermediate pixels and geometry remain assertions.
  const {extension,shadowControl} = resolveImageFilename(workflow.input.name);
  const document = new CoreSession().createDocument({...decodeImage(Buffer.from(workflow.input.bytes,'base64')),shadowControl});
  const automatic = document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  let selected = automatic.settings,page = 'background',completed = 0;
  compareDocumentState(automatic.state,workflow.expectedAutomatic.state,metadata.parameters);
  document.beginEditing();document.beginBackgroundEditing();
  compareFrame(document,selected,page,workflow.expectedFrames[0],workflow.id+' initial');
  for (const [index,action] of workflow.actions.entries()) {
    if (action.action === 6) {
      const event = {phase:'select',tool:tools[action.tool]};
      if (page === 'background') document.backgroundGesture(event);else document.gesture(event);
    } else if (action.phase) {
      const event = {phase:action.phase,point:action.point,buttons:action.buttons,button:action.button,modifiers:action.modifiers};
      if (page === 'background') document.backgroundGesture(event);else document.gesture(event);
    } else if (action.action === 10) { document.beginEditing();page = 'editing'; }
    else if (action.action === 11) document.gesture({phase:'color',color:action.argb.slice().reverse()});
    else if (action.action === 12) { document.beginBackgroundEditing();page = 'background'; }
    else if (action.action === 13) document.undoBackground();
    else if (action.action === 14) document.redoBackground();
    else if (action.action === 15) document.revertBackground();
    else if (action.action === 16) {
      const state = document.snapshot();
      assert.equal(findOutputRegion(state.segmentation,state.contours,action.point),workflow.expectedFrames[index+1].queryRegion,
        workflow.id+' point '+action.point);
    } else if (action.action === 18) {
      const input = workflow.additionalInputs[action.source],filename = resolveImageFilename(input.name);
      document.setSource({...decodeImage(Buffer.from(input.bytes,'base64')),shadowControl:filename.shadowControl});
      selected = {...selected,palette:new Float32Array(),backgroundMode:-1,background:Float32Array.of(1,1,1,0)};
      const replaced = document.runAutomatic({...selected,extension:filename.extension});selected = replaced.settings;
      compareDocumentState(replaced.state,workflow.expectedFrames[index+1].replacedSource.automatic.state,metadata.parameters);
      document.beginEditing();page = 'export';
    } else if (action.action === 19) {
      assert.ok(Object.hasOwn(properties,action.slot));
      const [key,convert] = properties[action.slot];selected = {...selected,[key]:convert(action.value)};
    } else if (action.action === 21) document.toggleBackground(action.point);
    else if (action.action === 22) {
      // These declared raw-record inputs describe the same point producer
      // exercised by the real-widget corpus; no recorded history is supplied.
      let point;
      if (action.kind === 3) {
        assert.deepEqual(action.coordinates.slice(0,2),action.coordinates.slice(2));point = action.coordinates.slice(0,2);
      } else point = [action.x+.5,action.y+.5];
      document.gesture({phase:'press',point,buttons:1,button:1});
    } else if (action.action === 2) document.undoEdit();
    else if (action.action === 3) document.redoEdit();
    else if (action.action === 4) document.revertEdits();
    else if (action.action === 5 || action.action === 23) {
      if (page === 'editing') {
        if (selected.advanced) document.commitEditing();else document.finishEditing(selected);
        page = selected.advanced ? 'smoothing' : 'export';
      } else if (page === 'smoothing') {
        const smoothed = document.runTask('smoothing',selected),fitted = document.runTask('fitting',selected);
        if (!smoothed.didWork && !fitted.didWork) { document.beginBackgroundEditing();page = 'background'; }
      } else { document.finishBackgroundEditing();page = 'export'; }
      compareDocumentState(document.snapshot(),workflow.expectedCompleted[completed++].state,metadata.parameters);
    } else assert.fail('Unknown native background action '+action.action);
    compareFrame(document,selected,page,workflow.expectedFrames[index+1],workflow.id+' event '+index);
  }
  assert.equal(completed,workflow.expectedCompleted.length);
});
