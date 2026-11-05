import assert from 'node:assert/strict';
import {test} from 'node:test';
import {isOutputRegionVisible,reverseCubicPoints,exportProgressPercent} from '../src/vector-output.mjs';

test('visibility masks honor word boundaries and visible tail',()=>{
  const words=new Uint32Array(3);words[0]=(1<<0)|(1<<31);words[1]=1;words[2]=1;
  for(const index of [0,31,32,64]) assert.equal(isOutputRegionVisible(65,words,index),false);
  for(const index of [1,30,33,63,65,100]) assert.equal(isOutputRegionVisible(65,words,index),true);
});
test('reversing a cubic twice restores all points without mutation',()=>{
  const points=[0,1,2,3,4,5,6,7],before=points.slice(),reversed=reverseCubicPoints(points);
  assert.deepEqual(reversed,[6,7,4,5,2,3,0,1]);assert.deepEqual(reverseCubicPoints(reversed),before);
  assert.deepEqual(points,before);
});
test('export progress is bounded and monotonic for every phase',()=>{
  const values=Array.from({length:51},(_,phase)=>exportProgressPercent(phase));
  assert.ok(values.every(value=>Number.isInteger(value)&&value>=0&&value<=100));
  assert.ok(values.every((value,index)=>index===0||value>=values[index-1]));
  assert.equal(values[0],6);assert.equal(values.at(-1),96);
});
