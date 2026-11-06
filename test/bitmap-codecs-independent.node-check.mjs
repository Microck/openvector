import assert from 'node:assert/strict';
import test from 'node:test';
import {encodeBmp, encodeTiff, encodePng, encodeJpeg} from '../src/bitmap-codecs.mjs';
import {decodeImage} from '../src/image-input.mjs';

const bgra = Uint8Array.from([0,0,255,255, 0,255,0,255, 255,0,0,255, 128,128,128,255]);
for (const [name, encode] of Object.entries({bmp:encodeBmp, tiff:encodeTiff, png:encodePng, jpeg:encodeJpeg})) {
  test(`${name} encodes authored pixels without modifying them`, () => {
    const input = Uint8Array.from(bgra), before = Uint8Array.from(input);
    const encoded = encode({width:2,height:2,bgra:input});
    assert.deepEqual(input,before);
    const decoded = decodeImage(encoded);
    assert.deepEqual([decoded.width,decoded.height],[2,2]);
    if (name !== 'jpeg') assert.deepEqual([...decoded.bgra],[...before]);
  });
  test(`${name} rejects invalid dimensions and channel values`, () => {
    assert.throws(()=>encode({width:0,height:1,bgra:[]}));
    assert.throws(()=>encode({width:2,height:1,bgra:new Uint8Array(4)}));
    assert.throws(()=>encode({width:1,height:1,bgra:[0,256,0,255]}));
  });
}
test('PNG stores explicit alpha and JPEG ignores alpha', () => {
  assert.equal(encodePng({width:2,height:2,bgra,hasAlpha:true})[25],6);
  assert.equal(encodePng({width:2,height:2,bgra})[25],2);
  const altered = Uint8Array.from(bgra);
  for (let i=3;i<altered.length;i+=4) altered[i]=i;
  assert.deepEqual(encodeJpeg({width:2,height:2,bgra:altered}),encodeJpeg({width:2,height:2,bgra}));
});
