import assert from 'node:assert/strict';
import {test} from 'node:test';
import {imageSize,validateBgra,normalizeCmykPixels,readPaletteColors} from '../src/pixel-input.mjs';

test('image size rejects invalid or overflowing dimensions',()=>{
  assert.equal(imageSize(4,3),12);
  for(const dimensions of [[0,1],[1,0],[-1,2],[1.5,2],[2147483647,2],[NaN,1]])
    assert.throws(()=>imageSize(...dimensions));
});
test('BGRA validation rejects incomplete or out-of-range channels',()=>{
  validateBgra(Uint8Array.from([0,127,255,255]));validateBgra([0,127,255,255]);
  for(const value of [null,[1,2,3],[0,0,0,256],[0,0,0,-1],[0,0,0,0.5]])
    assert.throws(()=>validateBgra(value));
});
test('palette colors are validated and owned',()=>{
  const input=[0,0.5,1,1],output=readPaletteColors(input);
  input[0]=1;assert.equal(output[0],0);
  for(const value of [[],[0,0,0],[0,0,0,NaN],[0,0,0,2]])
    assert.throws(()=>readPaletteColors(value));
});
test('CMYK conversion preserves opacity at black and white extremes',()=>{
  const bgra=Uint8Array.from([0,0,0,128,255,255,255,64]);
  normalizeCmykPixels(bgra,Uint8Array.from([0,255]));
  assert.deepEqual([...bgra],[255,255,255,128,0,0,0,64]);
});
