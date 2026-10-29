import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import test from 'node:test';
import { decodeImage, resolveImageFilename } from '../src/image-input.mjs';
import { prepareImageState } from '../src/image-pipeline.mjs';
import { CoreSession } from '../src/settings.mjs';
import { cleanStartupProfile,compareDocumentState,settingsFromBits } from './helpers/native-gui.mjs';

test('expanded BMP layouts and retained BMP inputs match native import and preparation',()=>{
  for (const fixture of ['image-input-reference','bmp-layout-input-reference','bmp-bitfields-input-reference','bmp-short-matrix-input-reference','bmp-header-variants-input-reference','bmp-rle-input-reference','bmp-rle-controls-input-reference','bmp-palettes-input-reference','bmp-wrappers-input-reference','bmp-ignored-masks-input-reference']) {
    const [metadata,...rows]=gunzipSync(readFileSync(new URL('./fixtures/'+fixture+'.ndjson.gz',import.meta.url)))
      .toString().trim().split('\n').map(JSON.parse);
    assert.equal(metadata.wholeNativeResponseRepeatIdentical,true);
    const cases=rows.filter(row=>row.format==='bmp');assert.ok(cases.length>0);
    for (const frame of cases) {
      const encoded=Buffer.from(frame.encoded,'base64'),before=Buffer.from(encoded),decoded=decodeImage(encoded);
      assert.equal(decoded.format,'bmp');assert.equal(decoded.hasAlpha,!!frame.decoded.matte,frame.id+' intrinsic alpha');
      assert.deepEqual([decoded.width,decoded.height],[frame.decoded.width,frame.decoded.height],frame.id);
      assert.deepEqual(Buffer.from(decoded.bgra),Buffer.from(frame.decoded.bgra,'base64'),frame.id+' decoded');
      const initial=prepareImageState({...decoded,mode:0,amount:0,parameters:{'Shared::is_anti_aliased':frame.antiAlias}});
      assert.deepEqual(Buffer.from(initial.source.bgra),Buffer.from(frame.source.bgra,'base64'),frame.id+' source');
      const prepared=prepareImageState({...decoded,mode:frame.mode,amount:frame.amount,parameters:{'Shared::is_anti_aliased':frame.antiAlias}});
      for (const [name,actual] of [['prepared',prepared.source],['working',prepared.working]]) {
        assert.deepEqual([actual.width,actual.height],[frame[name].width,frame[name].height],frame.id+' '+name+' dimensions');
        assert.deepEqual(Buffer.from(actual.bgra),Buffer.from(frame[name].bgra,'base64'),frame.id+' '+name);
      }
      assert.deepEqual(encoded,before,frame.id+' ownership');
    }
  }
});

test('BMP byte input preserves offset views and owns decoded storage',()=>{
  const [,reference]=gunzipSync(readFileSync(new URL('./fixtures/bmp-layout-input-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const encoded=Buffer.from(reference.encoded,'base64'),container=Buffer.concat([Buffer.of(1,2),encoded,Buffer.of(3)]);
  const view=container.subarray(2,-1),first=decodeImage(view),second=decodeImage(view);
  first.bgra.fill(0);assert.notDeepEqual(first.bgra,second.bgra);assert.deepEqual(view,encoded);
  for (const length of [4,14,25,encoded.length-1]) assert.throws(()=>decodeImage(encoded.subarray(0,length)));
  assert.deepEqual(view,encoded);
});

const [bmpConversionMetadata,...bmpConversions]=gunzipSync(readFileSync(new URL('./fixtures/bmp-expanded-conversion-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
for (const reference of bmpConversions) test('native complete BMP conversion: '+reference.id,()=>{
  assert.equal(bmpConversionMetadata.wholeFreshProcessResponsesRepeatIdentical,true);
  const decoded=decodeImage(Buffer.from(reference.input.bytes,'base64'));
  const {shadowControl,extension}=resolveImageFilename(reference.input.name);
  const document=new CoreSession().createDocument({...decoded,shadowControl});
  const automatic=document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  const native=reference.expectedAutomatic;
  assert.deepEqual(automatic.settings,settingsFromBits(native.selectedSettingsBits,native.selectedPaletteArgbBits));
  compareDocumentState(automatic.state,native.state,bmpConversionMetadata.parameters);
});
