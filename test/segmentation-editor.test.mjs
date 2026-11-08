import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {CoreSession} from '../src/settings.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {SegmentationEditor} from '../src/segmentation-editor.mjs';
import {cleanStartupProfile,compareDocumentState} from './helpers/native-gui.mjs';
import {exportSvg} from '../src/svg.mjs';

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

for (const {from,to,pixels} of [
  {from:[1.9375,2.5],to:[2.0625,2.5],pixels:[11,12]},
  {from:[2.0625,2.5],to:[1.9375,2.5],pixels:[11,12]},
  {from:[2.5,1.9375],to:[2.5,2.0625],pixels:[7,12]},
  {from:[1.99,1.99],to:[2.01,2.01],pixels:[6,12]},
]) test(`short pencil paints only endpoint pixels: ${from} to ${to}`,() => {
  const bgra = new Uint8Array(5*5*4).fill(255);
  const editor = new SegmentationEditor({width:5,height:5,bgra,source:bgra,
    backgroundMode:0,background:[0,0,0,1],color:[0,0,0,1]});
  const expected = bgra.slice();
  for (const pixel of pixels) expected.set([0,0,0,255],pixel*4);
  const painted = editor.apply({tool:'pencil',from,to,color:[0,0,0,1]});
  assert.deepEqual(painted.bgra,expected);
  assert.deepEqual(painted.undo.map(record => record.coordinates),[[...from,...to]]);
  assert.deepEqual(editor.undo().bgra,bgra);
  assert.deepEqual(editor.redo(),painted);
});

test('short pencil gestures retain local pixels and replayable history without no-op moves',() => {
  const bgra = new Uint8Array(5*5*4).fill(255),pointer = {previousPoint:[-1,-1]};
  const editor = new SegmentationEditor({width:5,height:5,bgra,source:bgra,
    backgroundMode:0,background:[0,0,0,1],color:[0,0,0,1]});
  editor.gesture({phase:'select',tool:'pencil'},pointer);
  const pressed = editor.gesture({phase:'press',point:[1.9375,2.5],buttons:1},pointer);
  const painted = editor.gesture({phase:'move',point:[2.0625,2.5],buttons:1},pointer);
  const expected = bgra.slice();
  expected.set([0,0,0,255],11*4);expected.set([0,0,0,255],12*4);
  assert.deepEqual(painted.bgra,expected);
  assert.deepEqual(painted.undo,[
    {kind:3,x:1,y:2,coordinates:[1.9375,2.5,1.9375,2.5],argb:[1,0,0,0],
      key:'3 1 2 1.9375 2.5 1.9375 2.5 [1, 0, 0, 0]'},
    {kind:3,x:2,y:2,coordinates:[2.0625,2.5,1.9375,2.5],argb:[1,0,0,0],
      key:'3 2 2 2.0625 2.5 1.9375 2.5 [1, 0, 0, 0]'},
  ]);
  assert.equal(painted.key,'3 1 2 1.9375 2.5 1.9375 2.5 [1, 0, 0, 0]3 2 2 2.0625 2.5 1.9375 2.5 [1, 0, 0, 0]');
  assert.deepEqual(editor.gesture({phase:'move',point:[2.125,2.5],buttons:1},pointer),painted);
  assert.deepEqual(pointer.previousPoint,[2.125,2.5]);
  const undone = editor.undo();
  assert.deepEqual(undone.bgra,pressed.bgra);
  assert.deepEqual(undone.undo,pressed.undo);
  assert.deepEqual(undone.redo,[painted.undo[1]]);
  assert.deepEqual(editor.redo(),painted);
});

function assertCompletedEditedFit(state,editor) {
  assert.equal(state.status,0);
  assert.equal(state.fitting.complete,true);
  function assertFinite(value) {
    if(typeof value==='number')assert.ok(Number.isFinite(value),'finite fitted coordinates');
    else if(value && typeof value==='object')for(const item of Object.values(value))assertFinite(item);
  }
  assertFinite(state.contours);
  assertFinite(state.smoothing);
  assertFinite(state.fitting.geometry);
  const {regions}=state.fitting.geometry,{indices,counts}=state.segmentation;
  assert.equal(regions.length,counts.length);
  const observed=new Array(regions.length).fill(0);
  for(const [pixel,region] of indices.entries()) {
    observed[region]++;
    assert.deepEqual(Array.from(regions[region].bgra),Array.from(editor.bgra.slice(pixel*4,pixel*4+4)),`painted pixel ${pixel}`);
    assert.deepEqual(regions[region].bgra,state.contours.regions[region].bgra);
  }
  assert.deepEqual(observed,Array.from(counts));
  assert.deepEqual(regions.map(region=>region.pixelCount),observed);
  const svg=exportSvg({geometry:state.fitting.geometry,width:editor.width,height:editor.height});
  assert.doesNotMatch(svg,/NaN|Infinity/);
  for(const {bgra} of regions)if(bgra[3]>10) {
    const rgb=[bgra[2],bgra[1],bgra[0]].map(channel=>channel.toString(16).padStart(2,'0')).join('');
    assert.ok(svg.includes('#'+rgb),'serialized painted color '+rgb);
  }
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
      if(record.id==='alpha-premultiplied') {
        assertCompletedEditedFit(state,document.editingState());
        assert.deepEqual(state.segmentation.palette,automatic.settings.palette,'exposed palette remains selected controls');
        assert.ok(Math.max(...state.segmentation.colorIndices)>=state.segmentation.palette.length/4,
          'painted slots survive beyond the shorter live palette');
      } else compareDocumentState(state,expected,metadata.parameters);
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

for (const id of ['palette-reorder','alpha-premultiplied']) for (const target of ['smoothing','fitting'])
test(`finishEditing ${id} stops on ${target} cancellation after committing edits and retries the same document`,()=>{
  const record=workflows.find(row=>row.id===id),source={...decodeImage(Buffer.from(record.input.bytes,'base64')),
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
  assertCompletedEditedFit(actual.snapshot(),editing);
  const complete=actual.snapshot();
  assert.deepEqual(actual.finishEditing(settings),complete,'cached finish retains palette, geometry and random state');
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
