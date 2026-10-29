import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { gunzipSync } from 'node:zlib';
import { CoreSession } from '../../src/palette.mjs';
import { decodeImage,resolveImageFilename } from '../../src/image-input.mjs';
import { exportSvg } from '../../src/svg.mjs';
import { exportPdf } from '../../src/pdf.mjs';
import { GuiEngine } from '../engine.mjs';
import { ProcessingJob } from '../processing-job.mjs';
import {encodeBmp} from '../../src/bitmap-codecs.mjs';
import {startIsolatedGui} from './gui-host.mjs';
import { settingsFromBits, restoredStartupProfile, cleanStartupProfile } from '../../test/helpers/native-gui.mjs';

const automaticReferences = gunzipSync(await readFile(new URL('../../test/fixtures/document-automatic-reference.ndjson.gz', import.meta.url)))
  .toString().trim().split('\n').map(line => JSON.parse(line));
const serializableSettings = settings => ({ ...settings, background: Array.from(settings.background), palette: Array.from(settings.palette) });
const [, ...editingReferences] = gunzipSync(await readFile(new URL('../../test/fixtures/segmentation-edit-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [, ...gestureReferences] = gunzipSync(await readFile(new URL('../../test/fixtures/segmentation-gesture-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const gestureTools = {0:'pan',3:'pencil',4:'picker',5:'fill',6:'zap'};

for (const reference of gestureReferences) test('GUI native pointer sequence: '+reference.id,() => {
  const gui = new GuiEngine(),opened = gui.open({name:reference.input.name,bytes:Buffer.from(reference.input.bytes,'base64')});
  gui.automatic({settings:opened.settings});gui.edit({action:'begin'});
  let completed = 0;
  for (const [index,action] of reference.actions.entries()) {
    let actual;
    if (action.action === 6) actual = gui.edit({action:'gesture',events:[{phase:'select',tool:gestureTools[action.tool]}]});
    else if (action.phase) actual = gui.edit({action:'gesture',events:[{phase:action.phase,point:action.point,
      button:action.button,buttons:action.buttons,modifiers:action.modifiers}]});
    else if (action.action === 5) {
      const finished = gui.edit({action:'finish'}),native = reference.expectedCompleted[completed++].state;
      assert.equal(finished.svg,exportSvg({geometry:native.fittedGeometry,width:finished.width,height:finished.height}));
      actual = gui.edit({action:'begin'});
    } else if (action.action === 10) actual = gui.edit({action:'begin'});
    else if (action.action === 11) actual = gui.edit({action:'gesture',events:[{phase:'color',color:action.argb.slice().reverse()}]});
    else actual = gui.edit({action:{2:'undo',3:'redo',4:'revert'}[action.action]});
    const expected = reference.expectedFrames[index+1];
    assert.deepEqual(actual.editor,{undoCount:expected.undo.length,redoCount:expected.redo.length,key:expected.key,
      tool:gestureTools[expected.inputMode],color:expected.paintArgb.slice().reverse()},reference.id+' GUI event '+index);
  }
  assert.equal(completed,reference.expectedCompleted.length);
});

test('GUI gesture batches preserve every native history entry and report invalid events',() => {
  const reference = gestureReferences.find(row => row.id === 'pencil-corners'),gui = new GuiEngine();
  const opened = gui.open({name:reference.input.name,bytes:Buffer.from(reference.input.bytes,'base64')});
  gui.automatic({settings:opened.settings});gui.edit({action:'begin'});
  const first = reference.actions.slice(0,7).map(action => action.action === 6 ? {phase:'select',tool:gestureTools[action.tool]}
    : {phase:action.phase,point:action.point,buttons:action.buttons,button:action.button,modifiers:action.modifiers});
  const actual = gui.edit({action:'gesture',events:first}),expected = reference.expectedFrames[7];
  assert.equal(actual.editor.key,expected.key);assert.equal(actual.editor.undoCount,expected.undo.length);
  assert.throws(() => gui.edit({action:'gesture',events:[]}),/nonempty/);
  assert.deepEqual(gui.state().editor,actual.editor);
  assert.throws(() => gui.edit({action:'gesture',events:[{phase:'move',point:[NaN,0],buttons:1}]}),/finite/);
  assert.deepEqual(gui.state().editor,actual.editor);
});

for (const reference of editingReferences.filter(record => record.referenceOutcome === 'completed-native-finite'))
  test('GUI editor and refit match native workflow: '+reference.id,() => {
    const gui = new GuiEngine();
    const opened = gui.open({name:reference.input.name,bytes:Buffer.from(reference.input.bytes,'base64')});
    let current = gui.automatic({settings:opened.settings});
    current = gui.edit({action:'begin'});
    const initialPaint = current.editor.color;
    for (const [index,action] of reference.actions.entries()) {
      if (action.action === 1) {
        const color = action.argb.slice().reverse();
        const command = action.kind === 3 ? {tool:'pencil',from:action.coordinates.slice(0,2),to:action.coordinates.slice(2),color,commit:action.commit}
          : {tool:action.kind === 5 ? 'fill' : 'zap',x:action.x,y:action.y,color,commit:action.commit};
        current = gui.edit({action:'apply',command});
      } else if (action.action === 5) {
        current = gui.edit({action:'finish'});
        assert.equal(current.editor,null);
        const native = reference.expectedCompleted[0].state;
        assert.equal(current.svg,exportSvg({geometry:native.fittedGeometry,width:current.width,height:current.height}));
        assert.equal(current.regionCount,native.segmentation.counts.length);
        continue;
      } else current = gui.edit({action:{2:'undo',3:'redo',4:'revert'}[action.action]});
      const expected = reference.expectedFrames[index+1],bgra = Buffer.from(expected.bgra,'hex'),rgba = new Uint8Array(bgra.length);
      for (let pixel = 0; pixel < bgra.length; pixel += 4) {
        const alpha = bgra[pixel+3];
        for (let channel = 0; channel < 3; channel++) rgba[pixel+channel] = alpha ? Math.round(bgra[pixel+2-channel]*255/alpha) : 0;
        rgba[pixel+3] = alpha;
      }
      assert.deepEqual(Buffer.from(current.segments.rgba,'base64'),Buffer.from(rgba),reference.id+' GUI pixels '+index);
      assert.deepEqual(current.editor,{undoCount:expected.undo.length,redoCount:expected.redo.length,key:expected.key,tool:'pan',color:initialPaint});
    }
    assert.deepEqual(Buffer.from(gui.export({format:'svg'}).bytes).toString(),current.svg);
  });

function bitmap() {
  const width = 16, height = 16, stride = width * 3;
  const bytes = Buffer.alloc(54 + stride * height);
  bytes.write('BM'); bytes.writeUInt32LE(bytes.length, 2); bytes.writeUInt32LE(54, 10);
  bytes.writeUInt32LE(40, 14); bytes.writeInt32LE(width, 18); bytes.writeInt32LE(height, 22);
  bytes.writeUInt16LE(1, 26); bytes.writeUInt16LE(24, 28); bytes.writeUInt32LE(stride * height, 34);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const channel = x >= 4 && x < 12 && y >= 4 && y < 12 ? 0 : 255;
    bytes.fill(channel, 54 + y * stride + x * 3, 54 + y * stride + x * 3 + 3);
  }
  return bytes;
}

test('GUI conversion and downloads use the real shared document and writers', () => {
  const bytes = bitmap(), gui = new GuiEngine();
  const opened = gui.open({ bytes, name: 'D:\\images\\square.bmp' });
  const controls = { ...opened.settings, category: 2, paletteMode: 2 };
  assert.equal(opened.file.name, 'square.bmp');
  assert.equal(Buffer.from(opened.bitmap.rgba, 'base64').length, 16 * 16 * 4);
  assert.equal(opened.settings.category, -1);
  const direct = new CoreSession().createDocument({...decodeImage(bytes),shadowControl:0});
  direct.runTask('segmentation', controls); direct.runTask('smoothing', controls);
  const fitted = direct.runTask('fitting', controls).state;
  const converted = gui.process({ settings: controls, through: 'fitting' });
  const options = { geometry: fitted.fitting.geometry, width: 16, height: 16 };
  assert.equal(converted.svg, exportSvg(options));
  assert.equal(converted.regionCount, fitted.segmentation.counts.length);
  assert.equal(converted.curveCount, 4, 'the square uses four cubic curves');
  assert.equal((converted.svg.match(/ C /g) ?? []).length, 4, 'those curves occur in the exported paths');
  assert.equal(Buffer.from(gui.export({ format: 'svg' }).bytes).toString(), converted.svg);
  assert.equal(gui.export({ format: 'svg' }).filename, 'square.svg');
  assert.deepEqual(Buffer.from(gui.export({ format: 'pdf' }).bytes), Buffer.from(exportPdf(options)));
  for (const format of ['eps', 'ai', 'dxf']) assert.ok(gui.export({ format }).bytes.length > 100);
  assert.throws(() => gui.open({ bytes: Buffer.from('not an image'), name: 'invalid.png' }), /Unsupported image format/);
  assert.equal(gui.state().svg, converted.svg, 'invalid files preserve the document');
  assert.throws(() => gui.process({ settings: controls, through: 'unknown' }), /final stage/);
  assert.throws(() => gui.export({ format: 'constructor' }), /Choose SVG/);
  assert.equal(gui.reset(), null); assert.equal(gui.state(), null);
  assert.throws(() => gui.export({ format: 'svg' }), /Open an image first/);
});

test('GUI palette candidate colors round-trip ARGB into document BGRA controls', () => {
  const bytes = bitmap(), gui = new GuiEngine();
  const opened = gui.open({ bytes, name: 'square.bmp' });
  const settings = { ...opened.settings, category: 2, paletteMode: 0 };
  const preview = gui.palettes({ settings, lower: 2, upper: 2 });
  const direct = new CoreSession().createDocument({...decodeImage(bytes),shadowControl:0});
  const nativeOrder = direct.runTask('palette', { ...settings, lower: 2, upper: 2 }).state.palettes[2];
  assert.deepEqual(preview.palettes[0].colors, [Array.from(nativeOrder.slice(0, 4)).reverse(), Array.from(nativeOrder.slice(4, 8)).reverse()]);
  const output = gui.process({ settings: { ...settings, palette: preview.palettes[0].colors.flat() }, through: 'fitting' });
  assert.ok(output.svg.includes('<path'));
  // Entering advanced mode preserves the unclassified source category.
  const advanced = {...opened.settings,advanced:true,paletteMode:2};
  assert.equal(advanced.category,-1);
  assert.equal(gui.palettes({settings:advanced,lower:2,upper:2}).settings.category,-1);
  assert.ok(gui.process({settings:advanced,through:'fitting'}).svg.includes('<path'));
});

test('loopback HTTP host serves the built GUI and calls its real processing worker', async t => {
  const host = await startIsolatedGui({ port: 0 });
  t.after(() => host.close());
  const api = (action, input) => fetch(`${host.url}/api/${action}`, input === undefined ? undefined : {
    method: 'POST', headers: { 'Content-Type': 'application/json', Origin: host.url }, body: JSON.stringify(input),
  });
  assert.equal((await fetch(host.url)).status, 200);
  assert.match(await (await fetch(host.url)).text(), /OpenVector/);
  assert.equal((await fetch(`${host.url}/api/state`, { headers: { Origin: 'https://unrelated.example' } })).status, 403);
  assert.equal((await api('unknown', {})).status, 404);
  assert.equal(await (await api('state')).json(), null);
  const upload = await api('open', { name: 'square.bmp', bytes: bitmap().toString('base64') });
  assert.equal(upload.status, 200);
  const opened = await upload.json();
  const converted = await api('process', { settings: { ...opened.settings, category: 2, paletteMode: 2 }, through: 'fitting' });
  assert.equal(converted.status, 200);
  const finished = await converted.json();
  const invalid = await api('process', { settings: { ...opened.settings, category: 2, quality: 99 }, through: 'fitting' });
  assert.equal(invalid.status, 400);
  const rejected = await invalid.json();
  assert.match(rejected.error, /quality/); assert.equal(rejected.state.svg, finished.svg);
  const output = await fetch(`${host.url}/api/export?format=svg&layout=stacked&thinStroke=false`);
  assert.match(output.headers.get('Content-Disposition'), /attachment; filename\*=UTF-8''square.svg/);
  assert.equal(output.status, 200); assert.equal(await output.text(), finished.svg);
  const automatic = await api('automatic', { settings: { ...opened.settings, advanced: true } });
  assert.equal(automatic.status, 200);
  const automated = await automatic.json();
  assert.equal(automated.settings.advanced, false);
  assert.equal(automated.stage, 'fitting'); assert.ok(automated.svg.includes('<path'));
  const editing = await api('edit',{action:'begin'});
  assert.equal(editing.status,200);assert.deepEqual((await editing.json()).editor,{undoCount:0,redoCount:0,key:'',tool:'pan',color:[1,1,1,1]});
  const firstMove=await api('edit',{action:'gesture',events:[{phase:'select',tool:'pencil'},
    {phase:'move',point:[3,3],buttons:1},{phase:'select',tool:'pencil'}]});
  assert.equal(firstMove.status,200);
  assert.deepEqual((await firstMove.json()).editor,{undoCount:0,redoCount:0,key:'',tool:'pan',color:[1,1,1,1]});
  const painted = await api('edit',{action:'apply',command:{tool:'fill',x:8,y:8,color:[0,0,1,1]}});
  const edited = await painted.json();assert.equal(painted.status,200);assert.equal(edited.editor.undoCount,1);
  const undone = await api('edit',{action:'undo'});assert.equal((await undone.json()).editor.redoCount,1);
  const redone = await api('edit',{action:'redo'});assert.equal((await redone.json()).editor.undoCount,1);
  await api('edit',{action:'revert'});
  const pointerReference = gestureReferences.find(row => row.id === 'pencil-corners');
  const pointerEvents = pointerReference.actions.slice(0,7).map(action => action.action === 6
    ? {phase:'select',tool:gestureTools[action.tool]}
    : {phase:action.phase,point:action.point,buttons:action.buttons,button:action.button,modifiers:action.modifiers});
  const streamed = await api('edit',{action:'gesture',events:pointerEvents});
  assert.equal(streamed.status,200);
  const streamedState = await streamed.json();
  assert.equal(streamedState.editor.key,pointerReference.expectedFrames[7].key);
  assert.equal(streamedState.editor.undoCount,pointerReference.expectedFrames[7].undo.length);
  const invalidGesture = await api('edit',{action:'gesture',events:[{phase:'select',tool:'fill'},
    {phase:'move',point:[null,0],buttons:1}]});
  assert.equal(invalidGesture.status,400);
  const rejectedGesture = await invalidGesture.json();
  assert.match(rejectedGesture.error,/finite/);
  assert.deepEqual(rejectedGesture.state.editor,{...streamedState.editor,tool:'fill'});
  const refitted = await api('edit',{action:'finish',settings:automated.settings});
  assert.equal(refitted.status,200);const updated = await refitted.json();assert.equal(updated.editor,null);assert.notEqual(updated.svg,automated.svg);
  assert.equal(await (await fetch(`${host.url}/api/export?format=svg`)).text(),updated.svg);
  const replacement = await api('open', { name: 'next.bmp', bytes: bitmap().toString('base64'),
    settings: { ...automated.settings, sensitivity: 2, smoothness: 9 } });
  const reloaded = await replacement.json();
  assert.equal(reloaded.settings.sensitivity, 2); assert.equal(reloaded.settings.smoothness, 9);
  assert.deepEqual(reloaded.settings.palette, []); assert.deepEqual(reloaded.settings.background, [1,1,1,0]);
  const badUpload = await api('open', { name: 'bad.bmp', bytes: bitmap().toString('base64'), settings: null });
  assert.equal(badUpload.status, 400);
  assert.deepEqual((await badUpload.json()).state.settings, reloaded.settings);
  assert.equal(await (await api('reset', null)).json(), null);
  assert.equal((await fetch(`${host.url}/api/export?format=svg`)).status, 400);
});

for (const id of ['native-gui-session', 'native-gui-transparent-session', 'native-gui-clean-session']) {
  test(`GUI automatic and retained controls match native session: ${id}`, () => {
    const reference = automaticReferences.find(row => row.id === id), gui = new GuiEngine();
    let first = true;
    for (const step of reference.steps) {
      if (step.action === 'load') {
        const startup = reference.profile === 'clean' ? cleanStartupProfile : restoredStartupProfile;
        const opened = gui.open({ name: step.source, bytes: Buffer.from(reference.inputs[step.source], 'base64'),
          ...(first && reference.profile !== 'clean' ? { settings: startup.controls } : {}) });
        assert.deepEqual(opened.settings, serializableSettings(settingsFromBits(step.expected.selectedSettingsBits, step.expected.selectedPaletteArgbBits)));
        first = false;
      } else {
        const result = gui.automatic({ settings: gui.state().settings });
        assert.deepEqual(result.settings, serializableSettings(settingsFromBits(step.expected.selectedSettingsBits, step.expected.selectedPaletteArgbBits)));
        assert.equal(result.svg, exportSvg({ geometry: step.expected.state.fittedGeometry,
          width: step.expected.state.segmentation.width, height: step.expected.state.segmentation.height }));
        assert.equal(result.regionCount, step.expected.state.segmentation.counts.length);
        assert.equal(result.stage, 'fitting');
      }
    }
  });
}

test('GUI palette HTTP requests match native counts 13..24 and reject 25 without mutation',async t => {
  const [,reference] = gunzipSync(await readFile(new URL('../../test/fixtures/advanced-palette-range-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const host = await startIsolatedGui({port:0});t.after(()=>host.close());
  const api = (action,input) => fetch(`${host.url}/api/${action}`,{method:'POST',
    headers:{'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'},
    body:JSON.stringify(input)});
  let response = await api('open',{name:reference.input.name,bytes:reference.input.bytes}),opened = await response.json();
  assert.equal(response.status,200);response = await api('automatic',{settings:opened.settings});
  assert.equal(response.status,200);const automatic = await response.json();
  let settings = {...automatic.settings,advanced:true,paletteMode:1},previous;
  for (const [index,expected] of reference.expectedCompleted.entries()) {
    const count = index+13;
    response = await api('palettes',{settings,lower:count,upper:count,count});assert.equal(response.status,200);
    const state = await response.json();
    assert.equal(state.committedCount,Buffer.from(expected.state.settingsBits,'hex').readInt32LE(8));
    assert.deepEqual(state.stageKeys,{palette:expected.state.cacheKey,...expected.state.stageKeys});
    assert.equal(state.svg,automatic.svg,'palette work retains the completed vector');
    for (const candidate of state.palettes) {
      const argb = Float32Array.from(candidate.colors.flatMap(color=>color.slice().reverse()));
      assert.equal(Buffer.from(argb.buffer).toString('hex'),expected.state.paletteBits[candidate.count]);
    }
    const candidate = state.palettes.find(candidate=>candidate.count===count);assert.ok(candidate);
    settings = {...settings,palette:candidate.colors.flat().map(value=>Math.fround(Math.round(value*65535)/65535))};
    previous = state;
  }
  response = await api('palettes',{settings,lower:25,upper:25});assert.equal(response.status,400);
  const rejected = await response.json();assert.match(rejected.error,/2 through 24/);
  assert.deepEqual(rejected.state,previous);
});

export { bitmap };

// The GUI owns job state; the document remains the single numerical owner.
// Compare interruption/retry against the native-verified direct document path.
for (const target of ['palette','segmentation','smoothing','fitting']) test(`single-image job forwards ${target} cancellation and retries its retained document`,() => {
  const cancellation = new Int32Array(new SharedArrayBuffer(4)),published = [];
  const job = new ProcessingJob({cancellation,publish:() => published.push(job.snapshot())});
  let armed = true;
  const gui = new GuiEngine({onProgress:event => {
    if (armed && !event.complete && event.stage === target) Atomics.store(cancellation,0,1);
    job.progress(event,event.complete);
    return job.signal.aborted;
  }});
  const bytes = bitmap(),opened = gui.open({name:'own.bmp',bytes});
  const settings = {...opened.settings,category:2,quality:1,paletteMode:2};
  const direct = new CoreSession().createDocument({...decodeImage(bytes),...resolveImageFilename('own.bmp')});
  direct.commitSettings(gui.snapshot().document.settings);
  const runGui = () => target === 'palette' ? gui.palettes({settings,lower:2,upper:4}) : gui.process({settings,through:'fitting'});
  const runDirect = () => {
    if (target === 'palette') return direct.analyze({...settings,lower:2,upper:4,
      onProgress:() => {if (armed) direct.requestCancel();}});
    for (const stage of ['segmentation','smoothing','fitting']) {
      const completed = direct.runTask(stage,{...settings,onProgress:() => {if (armed && stage === target) direct.requestCancel();}});
      if (completed.didWork && completed.state.status === 2) break;
    }
  };
  job.begin();
  assert.throws(runGui,{name:'CancelTaskException'});
  if (target === 'palette') assert.throws(runDirect,{name:'CancelTaskException'});else runDirect();
  job.finish('cancelled');
  assert.deepEqual(gui.snapshot().document,direct.snapshot(),'same partial numerical state');
  assert.deepEqual(Buffer.from(gui.snapshot().document.original.bgra),Buffer.from(decodeImage(bytes).bgra),'loaded source retained');
  assert.equal(job.snapshot().current.stage,target);
  assert.equal(job.snapshot().current.complete,false,'active checkpoint is not completion');
  assert.ok(published.some(state => state.status === 'cancelling'));
  assert.equal(gui.state().svg,null);
  armed = false;job.begin();runGui();runDirect();job.finish('complete');
  assert.equal(Atomics.load(cancellation,0),0,'retry clears the cancellation request');
  assert.deepEqual(gui.snapshot().document,direct.snapshot(),'same retained-stream retry');
  assert.equal(job.snapshot().status,'complete');
});

test('HTTP single-image progress and Cancel remain responsive during real worker processing', {timeout:120000},async t => {
  const host = await startIsolatedGui({port:0});t.after(() => host.close());
  const headers = {'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const api = (action,input) => fetch(host.url+'/api/'+action,input===undefined ? {headers} :
    {method:'POST',headers,body:JSON.stringify(input)});
  const width=256,height=256,colors=[[24,40,176,255],[201,167,35,255],[68,197,112,255],[183,36,153,255]];
  const bgra=Uint8Array.from({length:width*height*4},(_,offset) => {
    const pixel=Math.floor(offset/4),x=pixel%width,y=Math.floor(pixel/width);
    return colors[(Math.floor(x/8)%2)+2*(Math.floor(y/8)%2)][offset%4];
  });
  const opened=await (await api('open',{name:'own-grid.bmp',bytes:encodeBmp({width,height,bgra}).toString('base64')})).json();
  const controls={settings:{...opened.settings,category:2,quality:1,paletteMode:2},through:'fitting'};
  const pending=api('process',controls);
  let active;
  const deadline=performance.now()+10000;
  do {
    active=await (await api('processing')).json();
    if(active.running && active.current?.stage==='segmentation' && !active.current.complete)break;
    await new Promise(resolve => setTimeout(resolve,5));
  }while(performance.now()<deadline);
  assert.equal(active.running,true);assert.equal(active.current.stage,'segmentation');
  const began=performance.now();
  assert.equal((await api('select',{advanced:true})).status,409,'conflicting mutation is rejected');
  const cancelled=await (await api('processing',{command:'cancel'})).json();
  assert.ok(performance.now()-began<1000,'host acknowledges cancellation without waiting for CPU work');
  assert.equal(cancelled.status,'cancelling');
  const response=await pending;assert.equal(response.status,400);
  const stopped=await response.json();assert.equal(stopped.code,'PROCESSING_CANCELLED');
  assert.equal(stopped.state.file.name,'own-grid.bmp');assert.equal(stopped.state.svg,null);
  assert.equal(stopped.state.stageKeys.segmentation,'');
  const final=await (await api('processing')).json();assert.equal(final.status,'cancelled');assert.equal(final.running,false);
  const retried=await api('process',controls);assert.equal(retried.status,200);
  const complete=await retried.json();assert.ok(complete.svg);assert.equal(complete.file.name,'own-grid.bmp');
  assert.equal((await (await api('processing')).json()).status,'complete');
});

for(const target of ['smoothing','fitting'])test(`GUI edit finish retains committed editor state after ${target} cancellation and retry`,()=>{
  const cancellation=new Int32Array(new SharedArrayBuffer(4)),job=new ProcessingJob({cancellation,publish:()=>{}});
  let armed=false;
  const gui=new GuiEngine({onProgress:event=>{
    if(armed && !event.complete && event.stage===target)Atomics.store(cancellation,0,1);
    job.progress(event,event.complete);return job.signal.aborted;
  }}),bytes=bitmap(),opened=gui.open({name:'own.bmp',bytes});
  const settings={...opened.settings,category:2,quality:1,paletteMode:2};
  job.begin();gui.process({settings,through:'fitting'});job.finish('complete');
  const preview=gui.state().preview;
  gui.edit({action:'begin'});gui.edit({action:'gesture',events:[{phase:'select',tool:'pencil'}]});
  const edited=gui.edit({action:'apply',command:{tool:'fill',x:5,y:5,color:[0,0,1,1]}});
  assert.equal(edited.editor.tool,'pencil');assert.equal(edited.editor.undoCount,1);
  armed=true;job.begin();assert.throws(()=>gui.edit({action:'finish',settings}),{name:'CancelTaskException'});job.finish('cancelled');
  const interrupted=gui.state();
  assert.equal(interrupted.editor.tool,'pan');assert.equal(interrupted.editor.undoCount,1);
  assert.equal(interrupted.stageKeys.segmentationEdits,edited.editor.key);
  assert.equal(interrupted.segments.rgba,edited.segments.rgba);assert.deepEqual(interrupted.preview,preview);
  assert.equal(interrupted.svg,null);assert.equal(job.snapshot().current.stage,target);
  assert.equal(job.snapshot().current.complete,false);
  armed=false;job.begin();const retried=gui.edit({action:'finish',settings});job.finish('complete');
  assert.equal(retried.editor,null);assert.ok(retried.svg);assert.equal(retried.segmentationHistory.undoCount,1);
  assert.equal(retried.stageKeys.segmentationEdits,edited.editor.key);
});

test('HTTP edit finish exposes cancellation and retains the committed editor for retry',{timeout:120000},async t=>{
  const host=await startIsolatedGui({port:0});t.after(()=>host.close());
  const headers={'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const api=(action,input)=>fetch(host.url+'/api/'+action,input===undefined?{headers}:{method:'POST',headers,body:JSON.stringify(input)});
  const width=256,height=256,colors=[[24,40,176,255],[201,167,35,255],[68,197,112,255],[183,36,153,255]],
    bgra=Uint8Array.from({length:width*height*4},(_,offset)=>{
      const pixel=Math.floor(offset/4),x=pixel%width,y=Math.floor(pixel/width);
      return colors[(Math.floor(x/8)%2)+2*(Math.floor(y/8)%2)][offset%4];
    });
  const opened=await(await api('open',{name:'own-grid.bmp',bytes:encodeBmp({width,height,bgra}).toString('base64')})).json();
  const settings={...opened.settings,category:2,quality:1,paletteMode:2};
  assert.equal((await api('process',{settings,through:'fitting'})).status,200);
  await api('edit',{action:'begin'});
  await api('edit',{action:'gesture',events:[{phase:'select',tool:'pencil'}]});
  const edited=await(await api('edit',{action:'apply',command:{tool:'fill',x:0,y:0,color:[0,0,1,1]}})).json();
  assert.equal(edited.editor.undoCount,1);
  const pending=api('edit',{action:'finish',settings});let active;
  const deadline=performance.now()+10000;
  do {
    active=await(await api('processing')).json();
    if(active.running && active.current?.stage==='smoothing' && !active.current.complete)break;
    await new Promise(resolve=>setTimeout(resolve,5));
  }while(performance.now()<deadline);
  assert.equal(active.running,true);assert.equal(active.current.stage,'smoothing');
  assert.equal((await api('processing',{command:'cancel'})).status,200);
  const response=await pending;assert.equal(response.status,400);
  const stopped=await response.json();assert.equal(stopped.code,'PROCESSING_CANCELLED');
  assert.equal(stopped.state.editor.tool,'pan');assert.equal(stopped.state.editor.undoCount,1);
  assert.equal(stopped.state.stageKeys.segmentationEdits,edited.editor.key);assert.equal(stopped.state.svg,null);
  assert.deepEqual(stopped.state.preview,edited.preview);
  const retried=await api('edit',{action:'finish',settings});assert.equal(retried.status,200);
  const complete=await retried.json();assert.ok(complete.svg);assert.equal(complete.editor,null);
  assert.equal(complete.segmentationHistory.undoCount,1);
});
