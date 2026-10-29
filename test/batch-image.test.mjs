import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { gunzipSync } from 'node:zlib';
import { CoreSession } from '../src/palette.mjs';
import { decodeImage, resolveImageFilename } from '../src/image-input.mjs';
import { exportSvg } from '../src/svg.mjs';
import { compareDocumentComputation, settingsFromBits } from './helpers/native-gui.mjs';

const [provenance,...references] = gunzipSync(await readFile(
  new URL('./fixtures/batch-image-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

for (const reference of references) test('batch image computation and SVGs match original batch: '+reference.id,() => {
  const session=new CoreSession();let document,result;
  for (const source of reference.sources) {
    const decoded=decodeImage(Buffer.from(source.bytes,'base64')),metadata=resolveImageFilename(source.name);
    if (document) document.setSource({...decoded,shadowControl:metadata.shadowControl});
    else document=session.createDocument({...decoded,shadowControl:metadata.shadowControl});
    const stages=[];
    result=document.runBatchImage({...reference.controls,extension:metadata.extension},stage=>stages.push(stage));
    assert.deepEqual(stages,reference.expectedStages);
    assert.equal(result.state.progress,100,'this component ends at fitting completion');
    assert.equal(exportSvg({geometry:result.state.fitting.geometry,width:decoded.width,height:decoded.height,
      layout:'grouped-cutouts'}),source.expectedSvg,'native exported SVG');
  }
  const expected=reference.expectedDocument;
  assert.deepEqual(result.settings,settingsFromBits(expected.controls.selectedSettingsBits,expected.selectedPaletteArgbBits));
  assert.equal(result.paletteCount,reference.expectedStages.includes('palette') ?
    expected.selectedPaletteArgbBits.length/32 : null,'selected palette length or skipped selection');
  for (const [index,field] of ['category','quality','paletteMode','colorCount'].entries())
    if (index<expected.controls.documentClassification.length)
      assert.equal(result.state.classification[field],expected.controls.documentClassification[index],field+' recommendation');
  // Keep the native post-export value intact. Export dispatch is outside this
  // per-image component and has not reached progress/transition parity yet.
  compareDocumentComputation(result.state,expected.state,provenance.parameters);
});

test('batch controls validate before changing the document or consuming random state',() => {
  const source=references[0].sources[0],decoded=decodeImage(Buffer.from(source.bytes,'base64'));
  const document=new CoreSession().createDocument({...decoded,...resolveImageFilename(source.name)}),initial=document.snapshot();
  for (const input of [null,{}, {extension:'.png',category:-1},{extension:'.png',quality:4},
    {extension:'.png',paletteMode:4},{extension:'.png',colorCount:1},{extension:'.png',colorCount:13},
    {extension:'.png',backgroundMode:2},{extension:'.png',advanced:1},{extension:'.png',onProgress:true}]) {
    assert.throws(()=>document.runBatchImage(input));
    assert.deepEqual(document.snapshot(),initial);
  }
  assert.throws(()=>document.runBatchImage({extension:'.png'},true),/observer/);
  assert.deepEqual(document.snapshot(),initial);
});

test('batch observers receive owned snapshots and automatic alpha follows source replacement',() => {
  const reference=references.find(row=>row.id==='alpha-auto-background'),source=reference.sources[0];
  const decoded=decodeImage(Buffer.from(source.bytes,'base64')),metadata=resolveImageFilename(source.name);
  const session=new CoreSession(),document=session.createDocument({...decoded,shadowControl:metadata.shadowControl});
  const actual=document.runBatchImage({...reference.controls,extension:metadata.extension},(_stage,snapshot)=>{
    snapshot.settings.background.fill(0);snapshot.completed.fill(0);snapshot.original.bgra.fill(0);
  });
  compareDocumentComputation(actual.state,reference.expectedDocument.state,provenance.parameters);
  assert.equal(actual.sourceAlphaType,2);assert.equal(actual.settings.backgroundMode,0);
  const opaque={...decoded,bgra:decoded.bgra.slice()};
  for (let index=3;index<opaque.bgra.length;index+=4) opaque.bgra[index]=255;
  document.setSource({...opaque,shadowControl:metadata.shadowControl});
  const replaced=document.runBatchImage({...reference.controls,extension:metadata.extension});
  assert.equal(replaced.sourceAlphaType,0);
});

test('document export requires fitted geometry and returns an owned post-export snapshot',async () => {
  const source=references[0].sources[0],decoded=decodeImage(Buffer.from(source.bytes,'base64'));
  const document=new CoreSession().createDocument({...decoded,...resolveImageFilename(source.name)});
  const initial=document.snapshot();
  await assert.rejects(document.runExport(),/save operation/);
  await assert.rejects(document.runExport(()=>0),/fitted geometry/);
  assert.deepEqual(document.snapshot(),initial);
  const fitted=document.runBatchImage({extension:resolveImageFilename(source.name).extension});
  const exported=await document.runExport(onProgress=>{
    assert.equal(document.snapshot().progress,0,'native save initialization');
    assert.equal(exportSvg({geometry:fitted.state.fitting.geometry,width:decoded.width,height:decoded.height,
      layout:'grouped-cutouts',onProgress}),source.expectedSvg);
    return 0;
  });
  assert.equal(exported.progress,6);
  const committed=document.snapshot();exported.settings.background.fill(0);exported.completed.fill(0);
  assert.deepEqual(document.snapshot(),committed);
});
