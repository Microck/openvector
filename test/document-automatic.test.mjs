import assert from 'node:assert/strict';
import {createReadStream} from 'node:fs';
import {createInterface} from 'node:readline';
import {createGunzip} from 'node:zlib';
import test from 'node:test';
import {CoreSession,decodeImage,resolveImageFilename} from '../src/settings.mjs';
import {createAutomaticComparisonSession,compareAutomaticFrame,compareLoadedFrame,
  restoredStartupProfile,cleanStartupProfile} from './helpers/native-gui.mjs';

const rows = createInterface({input:createReadStream(new URL('./fixtures/document-automatic-reference.ndjson.gz',import.meta.url))
  .pipe(createGunzip()),crlfDelay:Infinity});
let referenceCount = 0;
for await (const line of rows) {
  const record = JSON.parse(line);
  if (record.type === 'metadata') {
    assert.equal(record.wholeNativeResponseRepeatIdentical,true);
    assert.deepEqual([record.cases,record.automaticActions,record.parameterComparisons],[13,36,3600]);
    continue;
  }
  test(`automatic document matches complete native GUI state: ${record.id}`, () => {
    const model = createAutomaticComparisonSession(record.profile==='clean' ? cleanStartupProfile : restoredStartupProfile);
    for (const step of record.steps) {
      if (step.action==='load') {
        const loaded = model.load(Buffer.from(record.inputs[step.source],'base64'),step.source);
        if (step.expected) compareLoadedFrame(loaded,step.expected);
      } else {
        const actual = model.runAutomatic();
        assert.equal(compareAutomaticFrame(actual,step.expected).parameterComparisons,100);
      }
    }
  });
  referenceCount++;
}
assert.equal(referenceCount,13);

// Input bytes come from the same own bitmap as the native clean session.
const fixtureRows = createInterface({input:createReadStream(new URL('./fixtures/document-automatic-reference.ndjson.gz',import.meta.url))
  .pipe(createGunzip()),crlfDelay:Infinity});
let clean;
for await (const line of fixtureRows) {
  const row=JSON.parse(line);if(row.id==='native-gui-clean-session'){clean=row;break;}
}
const encoded=Buffer.from(clean.inputs['ambivalence-gui-square.bmp'],'base64');
const createDocument=()=>new CoreSession().createDocument({...decodeImage(encoded),...resolveImageFilename('square.bmp')});

test('automatic defaults and owned stage observations preserve native clean output', () => {
  const document=createDocument(),reference=clean.steps.find(step=>step.action==='auto').expected;
  const result=document.runAutomatic({extension:'.bmp'},(stage,snapshot)=>{
    snapshot.classification.category=-1;snapshot.original.bgra.fill(0);snapshot.randomState=-1;
  });
  assert.equal(result.settings.advanced,false);
  const expectedSettings=reference.selectedSettingsBits;
  assert.equal(result.paletteCount,Buffer.from(expectedSettings,'hex').readInt32LE(8));
  assert.deepEqual(result.state.fitting.geometry,reference.state.fittedGeometry);
  assert.equal(result.state.randomState,reference.state.randomState);
  result.settings.palette.fill(0);result.state.original.bgra.fill(0);
  assert.notDeepEqual(document.snapshot().original.bgra,result.state.original.bgra);
});

test('automatic validation leaves the document and PRNG unchanged', () => {
  const document=createDocument(),before=document.snapshot();
  assert.throws(()=>document.runAutomatic({}),/extension/);
  assert.throws(()=>document.runAutomatic({extension:'.bmp',quality:9}),/quality/);
  assert.throws(()=>document.runAutomatic({extension:'.bmp'},true),/observer/);
  assert.throws(()=>document.runAutomatic({extension:'.bmp',onProgress:true}),/onProgress/);
  assert.deepEqual(document.snapshot(),before);
});

test('automatic progress observes the native pipeline without exposing its selected settings',() => {
  const document=createDocument(),baseline=createDocument().runAutomatic({extension:'.bmp'}),events=[];
  const result=document.runAutomatic({extension:'.bmp',onProgress:event => events.push(event)},(stage,state,selection) => {
    selection.settings.palette.fill(NaN);state.original.bgra.fill(0);
  });
  assert.ok(events.some(event => event.stage==='segmentation'));
  assert.ok(events.some(event => event.stage==='smoothing'));
  assert.ok(events.some(event => event.stage==='fitting'));
  assert.deepEqual(result,baseline);
});
