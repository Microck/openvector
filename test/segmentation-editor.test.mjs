import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {CoreSession} from '../src/settings.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {SegmentationEditor} from '../src/segmentation-editor.mjs';
import {cleanStartupProfile,compareDocumentState} from './helpers/native-gui.mjs';

const [metadata, ...workflows] = gunzipSync(readFileSync(new URL('./fixtures/segmentation-edit-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const automaticInputs = new Map();

// Only own encoded images and declared tool commands enter the independent
// implementation. Captured pixels/history are assertions, never base inputs.
function automatic(record) {
  const key = record.input.name+'\0'+record.input.sha256;
  if (!automaticInputs.has(key)) {
    const decoded = decodeImage(Buffer.from(record.input.bytes,'base64'));
    const {shadowControl,extension} = resolveImageFilename(record.input.name);
    const document = new CoreSession().createDocument({...decoded,shadowControl});
    automaticInputs.set(key,document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension}).state);
  }
  return automaticInputs.get(key);
}

function editorFromAutomatic(state) {
  const {width,height,indices,colorIndices,palette} = state.segmentation;
  const bgra = new Uint8Array(width*height*4);
  for (let pixel = 0; pixel < indices.length; pixel++) for (let channel = 0; channel < 4; channel++)
    bgra[pixel*4+channel] = Math.round(palette[colorIndices[indices[pixel]]*4+channel]*255);
  return new SegmentationEditor({width,height,bgra,source:state.original.bgra,
    backgroundMode:state.settings.backgroundMode,background:state.settings.background,color:palette.subarray(0,4)});
}

function command(action) {
  const color = action.argb.slice().reverse();
  return action.kind === 3 ? {tool:'pencil',from:action.coordinates.slice(0,2),to:action.coordinates.slice(2),color,commit:action.commit}
    : {tool:action.kind === 5 ? 'fill' : 'zap',x:action.x,y:action.y,color,commit:action.commit};
}

function compareFrame(actual,expected,label) {
  assert.deepEqual(Buffer.from(actual.bgra),Buffer.from(expected.bgra,'hex'),label+' pixels');
  assert.equal(actual.key,expected.key,label+' active key');
  for (const list of ['undo','redo']) assert.deepEqual(actual[list],expected[list].map(({bits,...record}) => record),label+' '+list);
  assert.deepEqual([actual.width,actual.height],expected.header.slice(3,5),label+' dimensions');
}

for (const record of workflows) test('native editor workflow: '+record.id,() => {
  const editor = editorFromAutomatic(automatic(record));
  compareFrame(editor.snapshot(),record.expectedFrames[0],record.id+' initial');
  for (const [index,action] of record.actions.entries()) {
    if (action.action === 1) editor.apply(command(action));
    else if (action.action === 2) editor.undo();
    else if (action.action === 3) editor.redo();
    else if (action.action === 4) editor.revert();
    // Next changes document regions and vectors, not this editor bitmap.
    compareFrame(editor.snapshot(),record.expectedFrames[index+1],record.id+' action '+index);
  }
});

test('editor owns snapshots and validates a complete command before mutation',() => {
  const editor = editorFromAutomatic(automatic(workflows[0]));
  const before = editor.snapshot();
  assert.throws(() => editor.apply({tool:'pencil',from:[1,2],to:[3,NaN],color:[0,0,0,1]}));
  assert.deepEqual(editor.snapshot(),before);
  const leaked = editor.apply({tool:'fill',x:8,y:8,color:[0,0,1,1]});
  const saved = editor.snapshot();
  leaked.bgra.fill(0); leaked.undo[0].argb.fill(0); leaked.undo.length = 0;
  assert.deepEqual(editor.snapshot(),saved);
});

// Preserve captured NaN payload/sign bits. They are expected-value codecs,
// not usable coordinates or permission to treat non-finite geometry as healthy.
function restoreReferenceFloats(value) {
  if (value && typeof value === 'object') {
    if (Object.hasOwn(value,'float64Bits')) return Buffer.from(value.float64Bits,'hex').readDoubleLE();
    if (Array.isArray(value)) return value.map(restoreReferenceFloats);
    return Object.fromEntries(Object.entries(value).map(([key,item]) => [key,restoreReferenceFloats(item)]));
  }
  return value;
}

function compareReferenceFloatBits(actual,expected,path = '') {
  if (!expected || typeof expected !== 'object') return;
  if (Object.hasOwn(expected,'float64Bits')) {
    const bits = Buffer.alloc(8);bits.writeDoubleLE(actual);
    assert.equal(bits.toString('hex'),expected.float64Bits,'captured non-finite bits at '+path);
    return;
  }
  for (const [key,value] of Object.entries(expected)) compareReferenceFloatBits(actual?.[key],value,path+'.'+key);
}

for (const record of workflows) test('native document after editing: '+record.id,() => {
  const decoded = decodeImage(Buffer.from(record.input.bytes,'base64'));
  const {shadowControl,extension} = resolveImageFilename(record.input.name);
  const document = new CoreSession().createDocument({...decoded,shadowControl});
  const automatic = document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  compareFrame(document.beginEditing(),record.expectedFrames[0],record.id+' initial');
  let completed = 0;
  for (const [index,action] of record.actions.entries()) {
    let editor;
    if (action.action === 1) editor = document.edit(command(action));
    else if (action.action === 2) editor = document.undoEdit();
    else if (action.action === 3) editor = document.redoEdit();
    else if (action.action === 4) editor = document.revertEdits();
    else {
      const state = document.finishEditing(automatic.settings);
      const expected = record.expectedCompleted[completed++].state;
      compareReferenceFloatBits(state.contours,expected.contours,'contours');
      compareReferenceFloatBits(state.fitting.geometry,expected.fittedGeometry,'fittedGeometry');
      compareDocumentState(state,restoreReferenceFloats(expected),metadata.parameters);
      editor = document.beginEditing();
    }
    compareFrame(editor,record.expectedFrames[index+1],record.id+' document action '+index);
  }
  assert.equal(completed,record.expectedCompleted.length);
});

test('alpha edit rebuild matches the native creation palette before profile reload',() => {
  const reference = JSON.parse(readFileSync(new URL('./fixtures/edited-alpha-palette-reference.json',import.meta.url),'utf8'));
  const record = workflows.find(row => row.id === 'alpha-premultiplied');
  assert.equal(record.input.sha256,reference.inputSha256);
  const {shadowControl,extension} = resolveImageFilename(record.input.name);
  const document = new CoreSession().createDocument({...decodeImage(Buffer.from(record.input.bytes,'base64')),shadowControl});
  document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  document.beginEditing();
  for (const action of record.actions.slice(0,-1)) {
    if (action.action === 1) document.edit(command(action));
    else if (action.action === 2) document.undoEdit();
    else if (action.action === 3) document.redoEdit();
    else if (action.action === 4) document.revertEdits();
  }
  assert.equal(record.actions.at(-1).action,5);
  const palette = document.commitEditing().segmentation.palette;
  assert.equal(Buffer.from(palette.buffer,palette.byteOffset,palette.byteLength).toString('hex'),reference.creationPaletteBgraBits);
  assert.equal(palette.length/4,3);
});

for (const target of ['smoothing','fitting']) test(`finishEditing stops on ${target} cancellation after committing edits and retries the same document`,()=>{
  const record=workflows.find(row=>row.id==='palette-reorder'),source={...decodeImage(Buffer.from(record.input.bytes,'base64')),
    ...resolveImageFilename(record.input.name)};
  const actual=new CoreSession().createDocument(source),direct=new CoreSession().createDocument(source);
  let settings;
  for(const document of [actual,direct]) {
    settings=document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension:source.extension}).settings;
    document.beginEditing();
    for(const action of record.actions.slice(0,-1)) {
      if(action.action===1)document.edit(command(action));
      else if(action.action===2)document.undoEdit();
      else if(action.action===3)document.redoEdit();
      else if(action.action===4)document.revertEdits();
      else assert.fail('fixture must end in one finish action');
    }
  }
  const editing=actual.editingState(),completed=[];
  assert.throws(()=>actual.finishEditing({...settings,onProgress:event=>{
    if(event.stage===target)actual.requestCancel();
  }},stage=>completed.push(stage)),{name:'CancelTaskException'});
  assert.deepEqual(completed,target==='smoothing'?[]:['smoothing']);
  direct.commitEditing();
  for(const stage of ['smoothing','fitting']) {
    const task=direct.runTask(stage,{...settings,onProgress:()=>{if(stage===target)direct.requestCancel();}});
    if(task.didWork && task.state.status===2)break;
  }
  assert.deepEqual(actual.snapshot(),direct.snapshot(),'same partial numerical state and random stream');
  assert.deepEqual(actual.editingState(),direct.editingState(),'actual committed input state');
  assert.equal(actual.snapshot().stageKeys.segmentationEdits,editing.key);
  assert.deepEqual(actual.editingHistory(),{undoCount:editing.undo.length,redoCount:editing.redo.length});
  assert.equal(actual.editingState().tool,'pan');
  assert.deepEqual(actual.finishEditing(settings),direct.finishEditing(settings),'same retained-state retry');
  assert.equal(actual.snapshot().fitting.complete,true);
});

test('finishEditing validates observers before committing pending edits',()=>{
  const record=workflows.find(row=>row.id==='palette-reorder'),source={...decodeImage(Buffer.from(record.input.bytes,'base64')),
    ...resolveImageFilename(record.input.name)},document=new CoreSession().createDocument(source);
  const {settings}=document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension:source.extension});
  document.beginEditing();document.edit(command(record.actions.find(action=>action.action===1)));
  const before={document:document.snapshot(),editor:document.editingState()};
  assert.throws(()=>document.finishEditing({...settings,onProgress:true}),/onProgress/);
  assert.throws(()=>document.finishEditing(settings,true),/observer/);
  assert.deepEqual({document:document.snapshot(),editor:document.editingState()},before);
});

test('finishEditing stage observers cannot mutate its returned document',()=>{
  const record=workflows.find(row=>row.id==='palette-reorder'),source={...decodeImage(Buffer.from(record.input.bytes,'base64')),
    ...resolveImageFilename(record.input.name)},document=new CoreSession().createDocument(source);
  const {settings}=document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension:source.extension});
  document.beginEditing();document.edit(command(record.actions.find(action=>action.action===1)));
  const stages=[],result=document.finishEditing(settings,(stage,completed)=>{
    stages.push(stage);completed.state.original.bgra.fill(0);completed.state.settings.background.fill(0);
  });
  assert.deepEqual(stages,['smoothing','fitting']);assert.deepEqual(result,document.snapshot());
});
