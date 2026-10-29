import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {decodeTiffPixels} from '../src/tiff-input.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {prepareImageState} from '../src/image-pipeline.mjs';
import {CoreSession} from '../src/settings.mjs';
import {cleanStartupProfile,compareDocumentState,settingsFromBits} from './helpers/native-gui.mjs';

for (const corpus of ['tiff','tiff-extended','tiff-quantum-matrix','tiff-layout-success','tiff-tile-variants','tiff-gray-tile-stride','tiff-sample-format','tiff-fax','tiff-storage-order','tiff-jpeg','tiff-jpeg-strips','tiff-jpeg-layout-success','tiff-jpeg-samples','tiff-jpeg-single-row','tiff-jpeg-gray-alpha','jpeg-alpha-modes','tiff-legacy-codecs','tiff-legacy-layout','tiff-thunder-commands','tiff-thunder-retained-row','tiff-thunder-initial-row','tiff-legacy-row-framing','tiff-predictor3','tiff-predictor3-layout','tiff-predictor3-fill-verified','tiff-fill-order-codecs-verified','tiff-predictor3-boundaries','tiff-predictor3-white','tiff-gray-padding','tiff-gray-padding32','tiff-gray-three','tiff-gray-planar-predictor3','tiff-packbits-tile-framing-verified']) test('native TIFF import and preparation: '+corpus,()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/'+corpus+'-input-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.wholeNativeResponseRepeatIdentical,true);assert.equal(cases.length,metadata.cases);
  for (const reference of cases) {
    const encoded=Buffer.from(reference.encoded,'base64'),before=Buffer.from(encoded);
    const raw=decodeTiffPixels(encoded),decoded=decodeImage(encoded);assert.equal(decoded.format,'tiff');
    assert.deepEqual([raw.width,raw.height,raw.colorSpace,raw.hasAlpha],
      [reference.decoded.width,reference.decoded.height,reference.decoded.colorSpace,!!reference.decoded.matte],reference.id+' dimensions/colorspace/matte');
    assert.deepEqual(Buffer.from(raw.bgra),Buffer.from(reference.decoded.bgra,'base64'),reference.id+' native PixelPacket bytes');
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

test('compressed multi-row planar strips retain original blob-reader failures',()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/tiff-planar-errors-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.cases,3);assert.equal(cases.length,3);
  for (const reference of cases) {
    assert.equal(reference.expectedFailures.length,2);
    assert.ok(reference.expectedFailures.every(run=>run.exitCode===11&&run.stage===921&&run.exceptionCode==='0xe06d7363'));
    assert.throws(()=>decodeImage(Buffer.from(reference.encoded,'base64')),/compressed planar strips/);
  }
});

test('TIFF byte detection preserves offset views and returns owned pixels',()=>{
  const [,reference]=gunzipSync(readFileSync(new URL('./fixtures/tiff-input-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const encoded=Buffer.from(reference.encoded,'base64'),container=Buffer.concat([Buffer.of(1,2),encoded,Buffer.of(3)]);
  const view=container.subarray(2,-1),first=decodeImage(view),second=decodeImage(view);
  first.bgra.fill(0);assert.notDeepEqual(first.bgra,second.bgra);assert.deepEqual(view,encoded);
  for (const length of [4,8,encoded.length-4]) assert.throws(()=>decodeImage(encoded.subarray(0,length)));
  assert.deepEqual(view,encoded);
});

test('JPEG TIFF generic raster overflow retains original failure evidence',()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/tiff-jpeg-process-errors-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.cases,2);assert.equal(cases.length,2);
  for (const reference of cases) {
    assert.equal(reference.expectedFailures.length,2);
    assert.ok(reference.expectedFailures.every(run=>run.exitCode===0xc0000374 ||
      run.exitCode===11&&run.stage===921&&run.exceptionCode==='0xc0000005'));
    assert.throws(()=>decodeImage(Buffer.from(reference.encoded,'base64')),/generic reader overruns its quantum buffer/);
  }
});

test('JPEG TIFF tables and frame marker failures match the original rejection boundary',()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/tiff-jpeg-malformed-errors-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.cases,2);assert.equal(cases.length,2);
  for (const reference of cases) {
    assert.equal(reference.expectedFailures.length,2);
    assert.ok(reference.expectedFailures.every(run=>run.exitCode===11&&run.stage===921&&run.exceptionCode==='0xe06d7363'));
    assert.throws(()=>decodeImage(Buffer.from(reference.encoded,'base64')),/TIFF JPEG (?:tables|block).*markers?/);
  }
});

test('unavailable old-JPEG TIFF compression retains the native rejection boundary',()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/tiff-old-jpeg-unavailable-errors-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.cases,18);assert.equal(cases.length,18);
  for (const reference of cases) {
    assert.equal(reference.expectedFailures.length,2);
    assert.ok(reference.expectedFailures.every(run=>run.exitCode===11&&run.stage===921&&run.exceptionCode==='0xe06d7363'));
    assert.throws(()=>decodeImage(Buffer.from(reference.encoded,'base64')),/Unsupported TIFF compression/);
  }
});

for (const corpus of ['tiff','tiff-jpeg','jpeg-alpha','tiff-legacy','tiff-predictor']) {
const [conversionMetadata,...conversions]=gunzipSync(readFileSync(new URL('./fixtures/'+corpus+'-conversion-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
for (const reference of conversions) test('native complete TIFF conversion: '+reference.id,()=>{
  assert.equal(conversionMetadata.wholeFreshProcessResponsesRepeatIdentical,true);
  const decoded=decodeImage(Buffer.from(reference.input.bytes,'base64'));
  const {shadowControl,extension}=resolveImageFilename(reference.input.name);
  const document=new CoreSession().createDocument({...decoded,shadowControl});
  const automatic=document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  const native=reference.expectedAutomatic;
  assert.deepEqual(automatic.settings,settingsFromBits(native.selectedSettingsBits,native.selectedPaletteArgbBits));
  compareDocumentState(automatic.state,native.state,conversionMetadata.parameters);
});
}


test('NeXT and ThunderScan malformed row commands retain native rejection boundaries',()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/tiff-legacy-row-framing-errors-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.cases,4);assert.equal(cases.length,4);
  for (const reference of cases) {
    assert.equal(reference.expectedFailures.length,2);
    assert.ok(reference.expectedFailures.every(run=>run.exitCode===11&&run.stage===921&&run.exceptionCode==='0xe06d7363'));
    assert.throws(()=>decodeImage(Buffer.from(reference.encoded,'base64')),/TIFF (?:NeXT|ThunderScan).*(?:incomplete|exceeds)/);
  }
});


test('TIFF predictor registration, sample types and tile delegates retain native rejection boundaries',()=>{
  for (const [corpus,count] of [['tiff-predictor3',70],['tiff-predictor-packbits-registration',16]]) {
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/'+corpus+'-errors-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.cases,count);assert.equal(cases.length,count);
  for (const reference of cases) {
    assert.equal(reference.expectedFailures.length,2);
    assert.ok(reference.expectedFailures.every(run=>run.exitCode===11&&run.stage===921&&run.exceptionCode==='0xe06d7363'));
    const reason=reference.id.includes('-tile-') ? /tile delegate.*sample depth/
      : reference.id.includes('-planar-rows-') ? /compressed planar strips/
      : reference.id.includes('-unregistered-') ? /predictor tag.*not registered/
      : reference.id.startsWith('tiff-predictor2-') ? /horizontal predictor sample type/ : /floating predictor sample type/;
    assert.throws(()=>decodeImage(Buffer.from(reference.encoded,'base64')),reason,reference.id);
  }
  }
});

test('TIFF incorrectly encoded FillOrder2 compressed streams retain native rejections',()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/tiff-predictor3-fill-encoding-errors-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.cases,8);assert.equal(cases.length,8);
  for (const reference of cases) {
    assert.ok(reference.expectedFailures.every(run=>run.exitCode===11&&run.stage===921&&run.exceptionCode==='0xe06d7363'));
    assert.throws(()=>decodeImage(Buffer.from(reference.encoded,'base64')),reference.id);
  }
});


test('TIFF PackBits scanline decoding rejects packets that cross native row boundaries',()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/tiff-packbits-row-boundary-errors-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.cases,8);assert.equal(cases.length,8);
  for (const reference of cases) {
    assert.equal(reference.expectedFailures.length,2);
    assert.ok(reference.expectedFailures.every(run=>run.exitCode===11&&run.stage===921&&run.exceptionCode==='0xe06d7363'));
    assert.throws(()=>decodeImage(Buffer.from(reference.encoded,'base64')),/TIFF PackBits packet crosses.*boundary/,reference.id);
  }
});
