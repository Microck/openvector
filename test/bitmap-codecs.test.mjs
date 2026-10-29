import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync,inflateSync} from 'node:zlib';
import test from 'node:test';
import {encodeBmp,encodeTiff,encodePng,encodeJpeg} from '../src/bitmap-codecs.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {CoreSession} from '../src/palette.mjs';
import {exportBitmap} from '../src/bitmap-export.mjs';

const references=['bitmap-codecs-reference','bitmap-jpeg-reference'].flatMap(name=>
  gunzipSync(readFileSync(new URL('./fixtures/'+name+'.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse));
const encoders={bmp:encodeBmp,tif:encodeTiff,png:encodePng,jpg:encodeJpeg};

for (const [format,encode] of Object.entries(encoders)) {
  for (const reference of references) {
    if (reference.format!==format) continue;
    test(`native bitmap ${format} bytes: ${reference.id}`,()=>{
      const {width,height,hasAlpha}=reference;
      const bgra=inflateSync(Buffer.from(reference.bgraZlib,'base64')),before=Buffer.from(bgra);
      const encoded=encode({width,height,bgra,hasAlpha});
      assert.ok(encoded.equals(Buffer.from(reference.encoded,'base64')),`encoded bytes differ for ${reference.id}`);
      assert.deepEqual(bgra,before);
      const decoded=decodeImage(encoded);
      assert.deepEqual([decoded.width,decoded.height],[width,height]);
      // JPEG owns exact encoded bytes above; it intentionally loses pixels.
      if (format!=='jpg') assert.deepEqual(Buffer.from(decoded.bgra),bgra);
    });
  }
  test(`${format} rejects incomplete pixels before encoding`,()=>{
    assert.throws(()=>encode({width:2,height:1,bgra:new Uint8Array(4)}),/one BGRA pixel/);
    assert.throws(()=>encode({width:0,height:1,bgra:[]}),/dimensions must be positive/);
    assert.throws(()=>encode({width:1,height:1,bgra:[0,256,0,255]}),/channels must be integers/);
  });
}

test('PNG preserves an explicit RGB/RGBA storage choice',()=>{
  const bgra=new Uint8Array([3,7,11,255]);
  assert.equal(encodePng({width:1,height:1,bgra}).readUInt8(25),2);
  assert.equal(encodePng({width:1,height:1,bgra,hasAlpha:true}).readUInt8(25),6);
  assert.throws(()=>encodePng({width:1,height:1,bgra,hasAlpha:1}),/hasAlpha must be a boolean/);
});

test('JPEG ignores supplied alpha and returns bytes owned across arena reuse',()=>{
  const reference=references.find(row=>row.id==='noise-large');
  const bgra=inflateSync(Buffer.from(reference.bgraZlib,'base64'));
  const first=encodeJpeg({...reference,bgra}),saved=Buffer.from(first);
  for (let index=3;index<bgra.length;index+=4) bgra[index]=index&255;
  assert.deepEqual(encodeJpeg({...reference,bgra}),saved);
  encodeJpeg({width:1,height:1,bgra:[255,255,255,255]});
  assert.deepEqual(first,saved);
  assert.throws(()=>encodeJpeg({width:65501,height:1,bgra:new Uint8Array(65501*4)}),/cannot exceed 65500/);
});

// Integration owns the complete decode/fit/scene/codec boundary. The primitive
// tests above still own encoding supplied pixels without modifying input.
const scenes=['bitmap-scenes-reference','bitmap-jpeg-scenes-reference','bitmap-visibility-reference'].flatMap(name=>
  gunzipSync(readFileSync(new URL('./fixtures/'+name+'.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse));
for (const reference of scenes) test('native fitted bitmap files: '+reference.id,()=>{
  const decoded=decodeImage(Buffer.from(reference.sourceBytes,'base64'));
  const {extension,shadowControl}=resolveImageFilename(reference.sourceFilename);
  const document=new CoreSession().createDocument({...decoded,shadowControl});
  document.runAutomatic({extension});
  for (const [,action,...point] of reference.actions??[]) {
    if (action===0) document.toggleBackground(point);
    else if (action===1) document.undoBackground();
    else if (action===2) document.redoBackground();
    else document.revertBackground();
  }
  const before=JSON.stringify(document.snapshot()),backgroundBefore=document.backgroundState();
  for (const [filename,expected] of Object.entries(reference.expected)) {
    const format=filename.slice(filename.lastIndexOf('.')+1);
    assert.ok(exportBitmap(document,{format,scale:reference.scale}).equals(Buffer.from(expected.bytes,'base64')),
      reference.id+' '+filename+' bytes differ');
  }
  assert.equal(JSON.stringify(document.snapshot()),before);
  assert.deepEqual(document.backgroundState(),backgroundBefore);
});

test('bitmap export requires explicit flatten consent and retains document state on rejection',()=>{
  const reference=scenes.find(row=>row.id==='alpha-png');
  const decoded=decodeImage(Buffer.from(reference.sourceBytes,'base64'));
  const {extension,shadowControl}=resolveImageFilename(reference.sourceFilename);
  const document=new CoreSession().createDocument({...decoded,shadowControl});document.runAutomatic({extension});
  const before=JSON.stringify(document.snapshot());
  for (const format of ['bmp','jpg','tif']) {
    assert.throws(()=>exportBitmap(document,{format}),error=>error.code==='BITMAP_FLATTEN_REQUIRED');
    assert.equal(JSON.stringify(document.snapshot()),before);
    const flattened=decodeImage(exportBitmap(document,{format,flatten:true}));
    assert.ok(flattened.bgra.every((channel,index)=>index%4!==3 || channel===255));
  }
  assert.throws(()=>exportBitmap(document,{format:'png',flatten:true}),/only.*non-PNG/);
  assert.throws(()=>exportBitmap(document,{format:'svg'}),/bitmap format/);
  assert.equal(JSON.stringify(document.snapshot()),before);
});
