import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {decodeJpegPixels} from '../src/jpeg-input.mjs';
import {prepareImageState} from '../src/image-pipeline.mjs';
import {CoreSession} from '../src/settings.mjs';
import {cleanStartupProfile,compareDocumentState,settingsFromBits} from './helpers/native-gui.mjs';

const corpora=['jpeg','jpeg-expanded-verified','jpeg-markers'];
for (const corpus of corpora) test('native JPEG import and source preparation: '+corpus,()=>{
  const [metadata,...cases]=gunzipSync(readFileSync(new URL('./fixtures/'+corpus+'-input-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  assert.equal(metadata.wholeNativeResponseRepeatIdentical,true);
  assert.equal(cases.length,metadata.cases);
  for (const reference of cases) {
    const encoded=Buffer.from(reference.encoded,'base64'),before=Buffer.from(encoded);
    const raw=decodeJpegPixels(encoded),decoded=decodeImage(encoded);
    assert.deepEqual([raw.width,raw.height,raw.colorSpace],
      [reference.decoded.width,reference.decoded.height,reference.decoded.colorSpace],reference.id+' native import dimensions/colorspace');
    assert.deepEqual(Buffer.from(raw.bgra),Buffer.from(reference.decoded.bgra,'base64'),reference.id+' original PixelPacket bytes');
    assert.equal(decoded.format,'jpeg');assert.equal(decoded.hasAlpha,false);
    const source=prepareImageState({...decoded,mode:0,amount:0,parameters:{'Shared::is_anti_aliased':reference.antiAlias}}).source;
    assert.deepEqual([source.width,source.height],[reference.source.width,reference.source.height],reference.id+' source dimensions');
    assert.deepEqual(Buffer.from(source.bgra),Buffer.from(reference.source.bgra,'base64'),reference.id+' RGB source normalization');
    const prepared=prepareImageState({...decoded,mode:reference.mode,amount:reference.amount,
      parameters:{'Shared::is_anti_aliased':reference.antiAlias}});
    for (const [name,actual] of [['prepared',prepared.source],['working',prepared.working]]) {
      assert.deepEqual([actual.width,actual.height],[reference[name].width,reference[name].height],reference.id+' '+name+' dimensions');
      assert.deepEqual(Buffer.from(actual.bgra),Buffer.from(reference[name].bgra,'base64'),reference.id+' '+name+' pixels');
    }
    assert.deepEqual(encoded,before,reference.id+' input ownership');
  }
});

test('JPEG byte detection accepts an offset view and rejects truncated input without changing source bytes',()=>{
  const [,reference]=gunzipSync(readFileSync(new URL('./fixtures/jpeg-input-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const encoded=Buffer.from(reference.encoded,'base64'),container=Buffer.concat([Buffer.of(1,2,3),encoded,Buffer.of(4)]);
  const view=container.subarray(3,-1),first=decodeImage(view),second=decodeImage(view);
  first.bgra.fill(0);assert.notDeepEqual(first.bgra,second.bgra);
  assert.deepEqual(view,encoded);
  for (const length of [2,4,12,encoded.length-8]) assert.throws(()=>decodeImage(encoded.subarray(0,length)));
  assert.deepEqual(view,encoded);
});

test('standalone two-component JPEG retains its explicit unsupported color boundary',()=>{
  // Original standalone output is unstable. TIFF owns the defined gray/alpha
  // interpretation; accepting its planes must not imply JPEG file alpha.
  const encoded=readFileSync(new URL('./fixtures/jpeg-two-component-input.jpg',import.meta.url));
  assert.throws(()=>decodeImage(encoded),/Unsupported JPEG component count/);
});

const [conversionMetadata,...conversions]=gunzipSync(readFileSync(new URL('./fixtures/jpeg-conversion-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
for (const reference of conversions) test('native complete JPEG conversion: '+reference.id,()=>{
  assert.equal(conversionMetadata.wholeFreshProcessResponsesRepeatIdentical,true);
  const decoded=decodeImage(Buffer.from(reference.input.bytes,'base64'));
  const {shadowControl,extension}=resolveImageFilename(reference.input.name);
  const document=new CoreSession().createDocument({...decoded,shadowControl});
  const automatic=document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  const native=reference.expectedAutomatic;
  assert.deepEqual(automatic.settings,settingsFromBits(native.selectedSettingsBits,native.selectedPaletteArgbBits));
  compareDocumentState(automatic.state,native.state,conversionMetadata.parameters);
});

