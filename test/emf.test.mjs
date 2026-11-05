import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync, createReadStream } from 'node:fs';
import { gunzipSync, createGunzip } from 'node:zlib';
import { createInterface } from 'node:readline';
import { exportEmf, serializeEmf } from '../src/emf.mjs';
import { createEmfDeviceMapper } from '../src/vector-output.mjs';
import { CoreSession } from '../src/palette.mjs';
import { defaultDocumentSettings } from '../src/document-controls.mjs';
import { CancelTaskException } from '../src/task-cancellation.mjs';
import cliReference from './fixtures/emf-cli-reference.json' with { type:'json' };
import { publicVectorFixture } from './helpers/native-vector-output.mjs';

test('buffered EMF emission matches original cancellation bytes and successful header rewrites', async () => {
  const [metadata,...corpus]=gunzipSync(readFileSync(new URL('./fixtures/export-interruption-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(line=>JSON.parse(line));
  const rows=corpus.filter(row=>row.request.format==='emf'),documents=new Map();
  // Device measurements are environment metadata, recovered from a completed
  // original header. Geometry always comes from the independent pixel pipeline.
  const deviceBytes=Buffer.from(rows.find(row=>row.request.cancelAt===0).expected.afterProcessExit,'base64');
  const referenceDevice={pixels:[deviceBytes.readInt32LE(72),deviceBytes.readInt32LE(76)],
    millimeters:[deviceBytes.readInt32LE(80),deviceBytes.readInt32LE(84)],
    micrometers:[deviceBytes.readInt32LE(100),deviceBytes.readInt32LE(104)]};
  const settings={...defaultDocumentSettings(),category:2,quality:1,sensitivity:1,advanced:true,antiAlias:false,
    complexity:6,minimumPixels:0,artifactRejection:2,clusterColors:false,smoothness:6,detectCorners:true,
    curveComplexity:6,smoothingAntiAlias:false,paletteMode:2,backgroundMode:1,background:Float32Array.of(1,1,1,1)};
  let completed=0,cancelled=0;
  for (const row of rows) {
    const source=metadata.inputs[row.source];
    if (!documents.has(row.source)) {
      assert.deepEqual(source.controls,[2,1,1,1,0,6,0,2,0,6,1,6,0,1,1,1,1,1]);
      const document=new CoreSession().createDocument({...source,bgra:Uint8Array.from(Buffer.from(source.bgra,'base64')),
        hasAlpha:false,shadowControl:0});
      for (const stage of ['segmentation','smoothing','fitting']) assert.equal(document.runTask(stage,settings).state.status,0);
      const state=document.snapshot();documents.set(row.source,{document,state,before:structuredClone(state.fitting.geometry)});
    }
    const {document,state}=documents.get(row.source),{layout,thinStroke,cancelAt}=row.request;
    const writes=[];
    let checkpoints=0,length=0,rewrites=0;
    const operation=document.runExport(onProgress=>{
      serializeEmf({geometry:state.fitting.geometry,width:source.width,height:source.height,referenceDevice,
        layout:['stacked','cutouts','grouped-cutouts'][layout],thinStroke:Boolean(thinStroke),
        onProgress:percent=>{if (++checkpoints===cancelAt) document.requestCancel();onProgress(percent);}},(bytes,position)=>{
          if (position===0 && writes.length>0) rewrites++;
          else {assert.equal(position,length,row.id);length+=bytes.length;}
          writes.push({bytes,position});
        });
      return 0;
    });
    if (cancelAt) {await assert.rejects(operation,CancelTaskException,row.id);assert.equal(checkpoints,cancelAt,row.id);cancelled++;}
    else {assert.equal((await operation).status,0,row.id);completed++;}
    const actual=new Uint8Array(length);
    for (const {bytes,position} of writes) actual.set(bytes,position);
    const expected=publicVectorFixture(Buffer.from(row.expected.afterProcessExit,'base64'));
    assert.ok(Buffer.from(actual).equals(expected),`${row.id}: ${length} actual bytes, ${expected.length} expected`);
    assert.equal(rewrites,cancelAt ? 0 : 1,row.id);
  }
  assert.deepEqual({completed,cancelled},{completed:108,cancelled:82});
  for (const {state,before} of documents.values()) assert.deepEqual(state.fitting.geometry,before);
  const input=cliReference.cases[0].input,failure=new Error('EMF consumer failed.');
  assert.throws(()=>serializeEmf(input,()=>{throw failure;}),error=>error===failure);
  let progress=0;
  assert.throws(()=>serializeEmf({...input,onProgress:()=>{progress++;}},null),TypeError);
  assert.equal(progress,0);
});

function inputFor(reference, frame) {
  const shape = reference.geometries[frame.geometry];
  return { ...shape, referenceDevice:reference.referenceDevice, layout:['stacked', 'cutouts', 'grouped-cutouts'][frame.shapeMode],
    thinStroke:frame.thinStroke, outputWidth:frame.outputWidth, outputHeight:frame.outputHeight,
    hiddenRegions:shape.geometry.regions.flatMap((_, region) => region < frame.hiddenSize &&
      (frame.hiddenWords[region >>> 5] & 1 << (region & 31)) ? [region] : []) };
}

test('EMF serialization matches all 22014 reference files with OpenVector metadata', async () => {
  const path = new URL('./fixtures/emf-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  const reference = { geometries:[] };
  let count = 0, summary;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { reference.referenceDevice = frame.referenceDevice; continue; }
    if (frame.type === 'geometry') {
      assert.equal(frame.index, reference.geometries.length); reference.geometries.push(frame.value); continue;
    }
    if (frame.type === 'summary') { summary = frame; continue; }
    assert.equal(frame.type, 'case');
    const actual = Buffer.from(exportEmf(inputFor(reference, frame))), expected = publicVectorFixture(Buffer.from(frame.expected, 'latin1'));
    if (!actual.equals(expected)) {
      let offset = 0;
      while (offset < Math.min(actual.length, expected.length) && actual[offset] === expected[offset]) offset++;
      assert.fail(`original EMF ${count}, byte ${offset}: ${actual.length} actual bytes, ${expected.length} expected bytes`);
    }
    count++;
  }
  assert.equal(count, 22014); assert.equal(reference.geometries.length, 3264);
  assert.deepEqual(summary, { type:'summary', cases:count, geometries:3264,
    recordingSha256:'6a4b5b3319e5094ccbb47c88bb214873e2eaeba6090e712734d0d0e8bbe20897' });
});

test('EMF device mapping and header frames match all 93880 original GDI boundary pairs', async () => {
  const path = new URL('./fixtures/emf-device-bounds-reference.ndjson.gz', import.meta.url);
  const rows = createInterface({ input:createReadStream(path).pipe(createGunzip()), crlfDelay:Infinity });
  let mapper, device;
  let count = 0, summary;
  for await (const line of rows) {
    const frame = JSON.parse(line);
    if (frame.type === 'metadata') { device = frame.referenceDevice; mapper = createEmfDeviceMapper(device); }
    else if (frame.type === 'summary') summary = frame;
    else if (frame.type === 'case') {
      const point = mapper.point(...frame.point);
      assert.deepEqual([Math.min(0, point[0]), Math.min(0, point[1]), Math.max(0, point[0]), Math.max(0, point[1])],
        frame.fillBounds, `original EMF device point ${count}`);
      assert.deepEqual(mapper.frame(frame.bounds), frame.frame, `original EMF frame ${count}`);
      count++;
    } else assert.equal(frame.type, 'metadata');
  }
  assert.equal(count, 93880);
  assert.deepEqual(summary, { type:'summary', cases:count, missingFills:0, referenceDevice:device });
});

test('EMF wide cubic records and wrapped source extents match all 402 original files', () => {
  const reference = JSON.parse(gunzipSync(readFileSync(new URL('./fixtures/emf-expanded-reference.json.gz', import.meta.url))));
  const header = Buffer.from(reference.cases[0].expected, 'latin1');
  reference.referenceDevice = { pixels:[header.readInt32LE(72),header.readInt32LE(76)],
    millimeters:[header.readInt32LE(80),header.readInt32LE(84)],
    micrometers:[header.readInt32LE(100),header.readInt32LE(104)] };
  assert.equal(reference.cases.length, 402);
  for (const [index, frame] of reference.cases.entries()) {
    const actual = Buffer.from(exportEmf(inputFor(reference, frame))), expected = publicVectorFixture(Buffer.from(frame.expected, 'latin1'));
    if (!actual.equals(expected)) {
      let offset = 0;
      while (offset < Math.min(actual.length,expected.length) && actual[offset] === expected[offset]) offset++;
      assert.fail(`expanded original EMF ${index}, byte ${offset}: ${actual.length} actual bytes, ${expected.length} expected bytes`);
    }
  }
});

test('EMF defaults preserve caller state and return independent binary buffers', () => {
  const input = structuredClone(cliReference.cases[0].input), snapshot = structuredClone(input);
  const defaults = { ...input, layout:undefined, thinStroke:undefined, outputWidth:undefined,
    outputHeight:undefined, hiddenRegions:undefined };
  const expected = publicVectorFixture(Buffer.from(cliReference.cases[0].expected, 'latin1')).toString('latin1');
  const first = exportEmf(defaults);
  assert.ok(first instanceof Uint8Array);
  assert.equal(Buffer.from(first).toString('latin1'), expected);
  first.fill(0);
  assert.equal(Buffer.from(exportEmf(defaults)).toString('latin1'), expected);
  assert.deepEqual(input, snapshot);
  assert.equal(Buffer.from(exportEmf({ ...input, outputWidth:79, outputHeight:61 })).toString('latin1'), expected);
});

test('EMF rejects invalid device measurements and incomplete graph/options', () => {
  const input = cliReference.cases[0].input;
  for (const referenceDevice of [undefined,null,{}, { ...input.referenceDevice, pixels:[0,1080] },
    { ...input.referenceDevice, millimeters:[1.5,296] }, { ...input.referenceDevice, micrometers:[1] },
    { ...input.referenceDevice, pixels:[2**31,1080] }, { ...input.referenceDevice, pixels:new Int32Array([1920,1080]) }]) {
    assert.throws(() => exportEmf({ ...input, referenceDevice }), /EMF.*referenceDevice/);
  }
  for (const invalid of [{}, { ...input, width:0 }, { ...input, layout:'unknown' },
    { ...input, outputWidth:undefined }, { ...input, geometry:{ ...input.geometry, nodes:[] } }]) {
    assert.throws(() => exportEmf(invalid), /EMF/);
  }
  assert.throws(() => exportEmf({ ...input, referenceDevice:{ ...input.referenceDevice,
    pixels:[0x7fffffff,0x7fffffff], millimeters:[1,1] } }), /EMF.*28\.4/);
});
