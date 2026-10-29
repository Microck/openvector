import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {rasterizePaths} from '../src/bitmap-raster.mjs';

const references=['raster-pixels-reference','raster-pixels-bands-reference','raster-pixels-pool-reference','raster-pixels-partial-reference','raster-pixels-quota-flushed-reference','raster-strokes-reference','raster-strokes-boundaries-reference'].flatMap(name=>
  gunzipSync(readFileSync(new URL('./fixtures/'+name+'.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse));
const commands=['M','L','C','Z'];

for (const reference of references) test('native raster pixels: '+reference.id,()=>{
  // Qt enums and wire opcodes belong only to fixture translation.
  const {width,height}=reference,hasAlpha=reference.format===6;
  const background=Array.from({length:4},(_,channel)=>(reference.fill>>>(channel*8))&255);
  const paths=reference.paths.map(path=>({color:path.bgra,fillRule:path.rule===1?'nonzero':'even-odd',stroke:path.rule===2,
    commands:path.commands.map(([kind,...values])=>[commands[kind],...values])}));
  const input={width,height,hasAlpha,background,transform:reference.matrix,paths};
  const before=JSON.stringify(input),actual=rasterizePaths(input);
  assert.ok(Buffer.from(actual.bgra).equals(Buffer.from(reference.expected.bgra,'base64')),reference.id+' pixel bytes differ');
  assert.deepEqual([actual.width,actual.height,actual.hasAlpha],[width,height,hasAlpha]);
  assert.equal(JSON.stringify(input),before);
});

test('raster path admission rejects invalid geometry before returning pixels',()=>{
  const frame={width:8,height:8,paths:[]};
  assert.throws(()=>rasterizePaths({...frame,transform:[Infinity,1,0,0]}),/four finite/);
  assert.throws(()=>rasterizePaths({...frame,paths:[{color:[0,0,0,255],commands:[['L',1,1]]}]}),/Start each raster subpath/);
  assert.throws(()=>rasterizePaths({...frame,paths:[{color:[0,0,0,255],commands:[['M',NaN,1]]}]}),/coordinates must be finite/);
  assert.throws(()=>rasterizePaths({...frame,paths:[{color:[0,0,0,255],commands:[['M',0,0],['L',1,1],['L',2,2]],stroke:true}]}),/one M followed by one L or C/);
});
