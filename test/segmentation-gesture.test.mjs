import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {CoreSession} from '../src/settings.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {cleanStartupProfile,compareDocumentState} from './helpers/native-gui.mjs';
import {replayApplicationEdits} from './helpers/application-edit-reference.mjs';
import {SegmentationEditor} from '../src/segmentation-editor.mjs';

const [metadata,...workflows] = gunzipSync(readFileSync(new URL('./fixtures/segmentation-gesture-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const tools = {0:'pan',3:'pencil',4:'picker',5:'fill',6:'zap'};

function compareEditor(actual,expected,label) {
  assert.deepEqual(Buffer.from(actual.bgra),Buffer.from(expected.bgra,'hex'),label+' pixels');
  assert.equal(actual.key,expected.key,label+' key');
  for (const name of ['undo','redo']) assert.deepEqual(actual[name],expected[name].map(({bits,...record}) => record),label+' '+name);
  if (expected.inputMode !== undefined) {
    assert.equal(actual.tool,tools[expected.inputMode],label+' tool');
    assert.deepEqual(Array.from(actual.color),expected.paintArgb.slice().reverse(),label+' paint');
  }
}

function assertFiniteGeometry(state) {
  assert.equal(state.status,0);
  assert.equal(state.fitting.complete,true);
  const visit = value => {
    if (typeof value === 'number') assert.ok(Number.isFinite(value),'finite geometry');
    else if (value && typeof value === 'object') for (const child of Object.values(value)) visit(child);
  };
  visit(state.contours);visit(state.smoothing);visit(state.fitting.geometry);
  const counts = new Array(state.segmentation.counts.length).fill(0);
  for (const region of state.segmentation.indices) counts[region]++;
  assert.deepEqual(counts,Array.from(state.segmentation.counts));
  assert.deepEqual(state.fitting.geometry.regions.map(region => region.pixelCount),counts);
}

// The short capture divides by zero in its original sampler. Use declared
// endpoints and reversible local pixel changes, never the recorded origin byte.
function shortStrokeOracle(initial) {
  let pixels = initial.bgra.slice(),point = [-1,-1];
  const undo = [],redo = [];
  return (before,after,action) => {
    if (action.action === 2 && undo.length) {
      const change = undo.pop();redo.unshift(change);pixels = change.before.slice();
    } else if (action.action === 3 && redo.length) {
      const change = redo.shift();undo.push(change);pixels = change.after.slice();
    } else if (before.tool === 'pencil' && (action.buttons&1) && ['press','move'].includes(action.phase)) {
      if (action.phase === 'press') point = action.point;
      const previous = pixels.slice(),color = Array.from(before.color,value => Math.round(value*65535)>>>8);
      for (const [x,y] of [point,action.point]) pixels.set(color,(Math.trunc(y)*initial.width+Math.trunc(x))*4);
      if (!pixels.every((value,index) => value === previous[index])) {
        undo.push({before:previous,after:pixels.slice()});redo.length = 0;
        assert.deepEqual(after.undo.at(-1).coordinates,[...action.point,...point]);
      }
      point = action.point;
    }
    assert.deepEqual(after.bgra,pixels,'stroke changes only endpoint-local pixels');
    assert.deepEqual(after.bgra.slice(0,4),initial.bgra.slice(0,4),'origin remains unchanged');
    assert.equal(after.undo.length,undo.length);assert.equal(after.redo.length,redo.length);
    assert.equal(after.key,after.undo.map(record => record.key).join(''));
  };
}

for (const workflow of workflows) test('native gesture sequence: '+workflow.id,() => {
  const decoded = decodeImage(Buffer.from(workflow.input.bytes,'base64'));
  const {shadowControl,extension} = resolveImageFilename(workflow.input.name);
  const document = new CoreSession().createDocument({...decoded,shadowControl});
  const automatic = document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  const initial = document.beginEditing();
  assert.equal(initial.tool,tools[workflow.initialMode]);
  assert.deepEqual(Array.from(initial.color),workflow.initialPaintArgb.slice().reverse());
  compareEditor(initial,workflow.expectedFrames[0],workflow.id+' initial');
  const short = workflow.id === 'pencil-short-samples' ? shortStrokeOracle(initial) : null;
  let completed = 0;
  for (const [index,action] of workflow.actions.entries()) {
    const before = document.editingState();
    let editor;
    if (action.action === 6) editor = document.gesture({phase:'select',tool:tools[action.tool]});
    else if (action.phase) editor = document.gesture({phase:action.phase,point:action.point,buttons:action.buttons,
      button:action.button,modifiers:action.modifiers});
    else if (action.action === 2) editor = document.undoEdit();
    else if (action.action === 3) editor = document.redoEdit();
    else if (action.action === 4) editor = document.revertEdits();
    else if (action.action === 10) editor = document.beginEditing();
    else if (action.action === 11) editor = document.gesture({phase:'color',color:action.argb.slice().reverse()});
    else if (action.action === 5) {
      const state = document.finishEditing(automatic.settings);
      const expected = workflow.expectedCompleted[completed++].state;
      if (short) {
        assertFiniteGeometry(state);
        const replay = replayApplicationEdits(workflow,{gesture:true});
        assert.deepEqual(state.fitting.geometry,replay.geometry,'complete deterministic refit');
      } else compareDocumentState(state,expected,metadata.parameters);
      editor = document.beginEditing();
    } else assert.fail('Unknown native gesture action '+action.action);
    if (short) {
      short(before,editor,action);
      assert.equal(editor.tool,tools[workflow.expectedFrames[index+1].inputMode]);
      assert.deepEqual(Array.from(editor.color),workflow.expectedFrames[index+1].paintArgb.slice().reverse());
      if (index < 5) compareEditor(editor,workflow.expectedFrames[index+1],workflow.id+' event '+index);
    }
    else compareEditor(editor,workflow.expectedFrames[index+1],workflow.id+' event '+index);
  }
  assert.equal(completed,workflow.expectedCompleted.length);
});

test('fresh Pencil movement advances the native outside-image anchor before its first press',()=>{
  const references=gunzipSync(readFileSync(new URL('./fixtures/editor-pointer-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.deepEqual(references.map(row=>row.stage),['cold','first-source','second-source']);
  for(const reference of references)assert.deepEqual(reference.pointer.previous,[-1,-1]);
  const workflow=workflows.find(row=>row.id==='pencil-corners');
  const {shadowControl,extension}=resolveImageFilename(workflow.input.name);
  const document=new CoreSession().createDocument({...decodeImage(Buffer.from(workflow.input.bytes,'base64')),shadowControl});
  document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  const initial=document.beginEditing();
  document.gesture({phase:'color',color:[0,0,1,1]});
  document.gesture({phase:'select',tool:'pencil'});
  const first=document.gesture({phase:'move',point:[3,3],buttons:1});
  assert.deepEqual(first.bgra,initial.bgra);assert.equal(first.undo.length,0);assert.equal(first.key,'');
  document.gesture({phase:'move',point:[10,9],buttons:0});
  const second=document.gesture({phase:'move',point:[5,3],buttons:1});
  const expected=initial.bgra.slice();
  for(const x of [3,4,5])expected.set([0,0,255,255],(3*initial.width+x)*4);
  assert.deepEqual(second.bgra,expected);assert.equal(second.undo.length,1);
  assert.deepEqual(second.undo[0].coordinates,[5,3,3,3]);
  const third=document.gesture({phase:'move',point:[7,3],buttons:1});
  for(const x of [6,7])expected.set([0,0,255,255],(3*initial.width+x)*4);
  assert.deepEqual(third.bgra,expected);assert.equal(third.undo.length,2);
  assert.deepEqual(third.undo[1].coordinates,[7,3,5,3]);
  assert.deepEqual(document.revertEdits().bgra,initial.bgra);
});

function pointerDocument(session=new CoreSession()) {
  const workflow=workflows.find(row=>row.id==='pencil-corners');
  const {shadowControl,extension}=resolveImageFilename(workflow.input.name);
  const source={...decodeImage(Buffer.from(workflow.input.bytes,'base64')),shadowControl};
  const document=session.createDocument(source);
  const automatic=document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  return {document,source,extension,settings:automatic.settings};
}

for(const replacement of ['source','reset','segmentation'])test('Pencil anchor survives '+replacement+' replacement',()=>{
  const {document,source,extension,settings}=pointerDocument();
  document.beginEditing();document.gesture({phase:'color',color:[0,0,1,1]});
  document.gesture({phase:'select',tool:'pencil'});
  document.gesture({phase:'press',point:[3,3],buttons:1});
  if(replacement==='segmentation') {
    const result=document.runTask('segmentation',{...settings,advanced:true,complexity:6,paletteMode:2});
    assert.equal(result.didWork,true);
  } else {
    if(replacement==='reset')document.reset();
    document.setSource(source);
    document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  }
  const initial=document.beginEditing();assert.equal(initial.undo.length,0);
  document.gesture({phase:'color',color:[0,0,1,1]});document.gesture({phase:'select',tool:'pencil'});
  const moved=document.gesture({phase:'move',point:[5,3],buttons:1});
  const expected=initial.bgra.slice();for(const x of [3,4,5])expected.set([0,0,255,255],(3*initial.width+x)*4);
  assert.deepEqual(moved.bgra,expected);assert.equal(moved.undo.length,1);
  assert.deepEqual(moved.undo[0].coordinates,[5,3,3,3]);
});

for(const action of ['gesture','command'])test('background '+action+' has the native shared-anchor effect',()=>{
  const {document}=pointerDocument();
  const initial=document.beginEditing();document.gesture({phase:'color',color:[0,0,1,1]});
  document.beginBackgroundEditing();
  if(action==='gesture')document.backgroundGesture({phase:'press',point:[3,3],buttons:1});
  else document.toggleBackground([3,3]);
  document.beginEditing();document.gesture({phase:'select',tool:'pencil'});
  const moved=document.gesture({phase:'move',point:[5,3],buttons:1});
  const expected=initial.bgra.slice();
  if(action==='gesture')for(const x of [3,4,5])expected.set([0,0,255,255],(3*initial.width+x)*4);
  assert.deepEqual(moved.bgra,expected);assert.equal(moved.undo.length,action==='gesture'?1:0);
  if(action==='gesture')assert.deepEqual(moved.undo[0].coordinates,[5,3,3,3]);
});

test('an outside background press updates the anchor even without a visibility change',()=>{
  const {document}=pointerDocument();
  document.beginEditing();document.gesture({phase:'color',color:[0,0,1,1]});
  document.gesture({phase:'select',tool:'pencil'});document.gesture({phase:'press',point:[3,3],buttons:1});
  document.beginBackgroundEditing();const before=document.backgroundState();
  assert.deepEqual(document.backgroundGesture({phase:'press',point:[-4,3],buttons:1}),before);
  const initial=document.beginEditing();document.gesture({phase:'select',tool:'pencil'});
  const moved=document.gesture({phase:'move',point:[5,3],buttons:1});
  assert.deepEqual(moved.bgra,initial.bgra);assert.equal(moved.undo.length,initial.undo.length);
});

test('documents sharing a random session keep independent pointer contexts',()=>{
  const session=new CoreSession(),first=pointerDocument(session).document,second=pointerDocument(session).document;
  first.beginEditing();first.gesture({phase:'select',tool:'pencil'});
  first.gesture({phase:'press',point:[3,3],buttons:1});
  const initial=second.beginEditing();second.gesture({phase:'color',color:[0,0,1,1]});
  second.gesture({phase:'select',tool:'pencil'});
  const moved=second.gesture({phase:'move',point:[5,3],buttons:1});
  assert.deepEqual(moved.bgra,initial.bgra);assert.equal(moved.undo.length,0);
});

test('application color entry converts automatic bytes once and retains a straight undo base',() => {
  const bgra = Uint8Array.of(50,147,83,191,160,32,133,223,7,8,9,0,30,40,50,255);
  const editor = new SegmentationEditor({width:2,height:2,bgra,source:bgra,
    backgroundMode:0,background:[0,0,0,1],color:Array.from(bgra.slice(0,4),value => value/255)});
  const raw = editor.snapshot();
  assert.deepEqual(raw.bgra,bgra);
  editor.enableStraightColors();
  const straight = editor.snapshot();
  assert.deepEqual(straight.bgra,Uint8Array.of(67,196,111,191,183,37,152,223,0,0,0,0,30,40,50,255));
  assert.deepEqual(straight.color,Float32Array.from([67,196,111,191],value => value/255));
  const painted = editor.apply({tool:'pencil',from:[0,0],to:[0,0],color:[1,.25,.5,.5]});
  assert.deepEqual(painted.bgra.slice(0,4),Uint8Array.of(255,64,128,128));
  const checkpoint = editor.historyCheckpoint();
  editor.enableStraightColors();
  assert.deepEqual(editor.snapshot(),painted,'re-entry never converts user paint');
  assert.deepEqual(editor.undo().bgra,straight.bgra);
  editor.enableStraightColors();
  assert.deepEqual(editor.redo(),painted);
  assert.deepEqual(editor.revert().bgra,straight.bgra);
  assert.deepEqual(editor.restoreCheckpoint(checkpoint),painted);
});

test('application color entry rejects switching a raw editor with undo or redo records',() => {
  for (const undo of [false,true]) {
    const bgra = Uint8Array.of(50,147,83,191);
    const editor = new SegmentationEditor({width:1,height:1,bgra,source:bgra,
      backgroundMode:0,background:[0,0,0,1],color:[0,0,0,1]});
    editor.apply({tool:'fill',x:0,y:0,color:[1,0,0,1]});
    if (undo) editor.undo();
    const before = editor.snapshot();
    assert.throws(() => editor.enableStraightColors(),/Cannot change color representation/);
    assert.deepEqual(editor.snapshot(),before);
  }
});

test('application document preserves straight painted colors through refit, re-entry and checkpoints',() => {
  const workflow = workflows.find(row => row.id === 'picker-alpha');
  const {shadowControl,extension} = resolveImageFilename(workflow.input.name);
  const document = new CoreSession().createDocument({...decodeImage(Buffer.from(workflow.input.bytes,'base64')),shadowControl});
  const automatic = document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  const raw = document.beginEditing(),straight = document.beginEditingForOutput();
  const expected = raw.bgra.slice();
  for (let offset = 0; offset < expected.length; offset += 4) {
    const alpha = expected[offset+3];
    for (let channel = 0; channel < 3; channel++) expected[offset+channel] = alpha === 0 ? 0
      : Math.min(255,Math.round(expected[offset+channel]*255/alpha));
  }
  assert.deepEqual(straight.bgra,expected);
  const presented = document.presentationGeometry();
  for (const [pixel,region] of automatic.state.segmentation.indices.entries())
    assert.deepEqual(Array.from(presented.regions[region].bgra),Array.from(expected.slice(pixel*4,pixel*4+4)));
  assert.deepEqual(document.beginEditingForOutput(),straight);
  const painted = document.edit({tool:'pencil',from:[1,1],to:[1,1],color:[1,.25,.5,.5]});
  const state = document.finishEditing(automatic.settings);
  assertFiniteGeometry(state);
  const geometry = document.presentationGeometry();
  assert.deepEqual(geometry,state.fitting.geometry,'edited fit is already straight');
  for (const [pixel,region] of state.segmentation.indices.entries())
    assert.deepEqual(Array.from(geometry.regions[region].bgra),Array.from(painted.bgra.slice(pixel*4,pixel*4+4)));
  assert.deepEqual(document.beginEditingForOutput().bgra,painted.bgra);
  assert.deepEqual(document.undoEdit().bgra,straight.bgra);
  assert.deepEqual(document.restoreCommittedEditing().bgra,painted.bgra);
  assert.deepEqual(document.redoEdit().bgra,painted.bgra);
  document.revertEdits();
  const reverted = document.finishEditing(automatic.settings);
  assertFiniteGeometry(reverted);
  assert.equal(reverted.stageKeys.segmentationEdits,'');
  assert.deepEqual(document.presentationGeometry(),reverted.fitting.geometry,'empty history does not imply automatic color ownership');
});
