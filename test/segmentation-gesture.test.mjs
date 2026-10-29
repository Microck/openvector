import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {CoreSession} from '../src/settings.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {cleanStartupProfile,compareDocumentState} from './helpers/native-gui.mjs';

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

for (const workflow of workflows) test('native gesture sequence: '+workflow.id,() => {
  const decoded = decodeImage(Buffer.from(workflow.input.bytes,'base64'));
  const {shadowControl,extension} = resolveImageFilename(workflow.input.name);
  const document = new CoreSession().createDocument({...decoded,shadowControl});
  const automatic = document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  const initial = document.beginEditing();
  assert.equal(initial.tool,tools[workflow.initialMode]);
  assert.deepEqual(Array.from(initial.color),workflow.initialPaintArgb.slice().reverse());
  compareEditor(initial,workflow.expectedFrames[0],workflow.id+' initial');
  let completed = 0;
  for (const [index,action] of workflow.actions.entries()) {
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
      compareDocumentState(document.finishEditing(automatic.settings),workflow.expectedCompleted[completed++].state,metadata.parameters);
      editor = document.beginEditing();
    } else assert.fail('Unknown native gesture action '+action.action);
    compareEditor(editor,workflow.expectedFrames[index+1],workflow.id+' event '+index);
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
