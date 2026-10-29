import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {prepareImageState} from '../src/image-pipeline.mjs';
import {CoreSession} from '../src/settings.mjs';
import {cleanStartupProfile,compareDocumentState,settingsFromBits} from './helpers/native-gui.mjs';

for (const corpus of ['gif','gif-opaque-gray','gif-structure-verified']) test('native GIF import and preparation: '+corpus,()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/'+corpus+'-input-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.wholeNativeResponseRepeatIdentical,true);
  assert.equal(cases.length,metadata.cases);
  for (const reference of cases) {
    const encoded=Buffer.from(reference.encoded,'base64'),before=Buffer.from(encoded),decoded=decodeImage(encoded);
    assert.equal(decoded.format,'gif');
    assert.deepEqual([decoded.width,decoded.height,decoded.hasAlpha],
      [reference.decoded.width,reference.decoded.height,!!reference.decoded.matte],reference.id+' dimensions and intrinsic matte');
    assert.deepEqual(Buffer.from(decoded.bgra),Buffer.from(reference.decoded.bgra,'base64'),reference.id+' decoded pixels');
    const source=prepareImageState({...decoded,mode:0,amount:0,parameters:{'Shared::is_anti_aliased':reference.antiAlias}}).source;
    assert.deepEqual(Buffer.from(source.bgra),Buffer.from(reference.source.bgra,'base64'),reference.id+' source pixels');
    const prepared=prepareImageState({...decoded,mode:reference.mode,amount:reference.amount,
      parameters:{'Shared::is_anti_aliased':reference.antiAlias}});
    for (const [name,actual] of [['prepared',prepared.source],['working',prepared.working]]) {
      assert.deepEqual([actual.width,actual.height],[reference[name].width,reference[name].height],reference.id+' '+name+' dimensions');
      assert.deepEqual(Buffer.from(actual.bgra),Buffer.from(reference[name].bgra,'base64'),reference.id+' '+name+' pixels');
    }
    assert.deepEqual(encoded,before,reference.id+' input ownership');
  }
});

test('tiny interlaced GIFs retain the original loader failure',()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/gif-interlace-errors-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.cases,3);assert.equal(cases.length,3);
  for (const input of cases) {
    const runs=input.expectedFailures;
    assert.equal(runs.length,2);
    assert.ok(runs.every(run=>run.exitCode===11&&run.stage===921&&run.exceptionCode==='0xe06d7363'));
    assert.throws(()=>decodeImage(Buffer.from(input.encoded,'base64')),/interlace selects a row outside/);
  }
});

test('GIF byte detection preserves an offset view and returns owned pixels',()=>{
  const [,reference]=gunzipSync(readFileSync(new URL('./fixtures/gif-input-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const encoded=Buffer.from(reference.encoded,'base64'),container=Buffer.concat([Buffer.of(1,2),encoded,Buffer.of(3)]);
  const view=container.subarray(2,-1),first=decodeImage(view),second=decodeImage(view);
  first.bgra.fill(0);assert.notDeepEqual(first.bgra,second.bgra);assert.deepEqual(view,encoded);
  for (const length of [6,12,encoded.length-4]) assert.throws(()=>decodeImage(encoded.subarray(0,length)));
  assert.deepEqual(view,encoded);
});

const [conversionMetadata,...conversions]=gunzipSync(readFileSync(new URL('./fixtures/gif-conversion-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
for (const reference of conversions) test('native complete GIF conversion: '+reference.id,()=>{
  assert.equal(conversionMetadata.wholeFreshProcessResponsesRepeatIdentical,true);
  const decoded=decodeImage(Buffer.from(reference.input.bytes,'base64'));
  const {shadowControl,extension}=resolveImageFilename(reference.input.name);
  const document=new CoreSession().createDocument({...decoded,shadowControl});
  const automatic=document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  const native=reference.expectedAutomatic;
  assert.deepEqual(automatic.settings,settingsFromBits(native.selectedSettingsBits,native.selectedPaletteArgbBits));
  compareDocumentState(automatic.state,native.state,conversionMetadata.parameters);
});
