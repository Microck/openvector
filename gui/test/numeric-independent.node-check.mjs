import test from 'node:test';
import assert from 'node:assert/strict';
import {createNumericText,formatFixedOne,isNumericSpace} from '../numeric-text.mjs';
const locale={decimal:',',group:'.',zero:'0',positive:'+',negative:'-',exponent:'e'};
test('numeric parsing accepts locale and fallback decimal symbols',()=>{
  const parser=createNumericText(locale);
  assert.deepEqual(parser.double('  -1,5  '),{value:-1.5,ok:true});
  assert.deepEqual(parser.double('1.5'),{value:1.5,ok:true});
  assert.deepEqual(parser.percent('25%'),{value:0.25,ok:true});
  assert.deepEqual(parser.integer('2147483648'),{value:0,ok:false});
  assert.deepEqual(parser.integer('-2147483648'),{value:-2147483648,ok:true});
  assert.deepEqual(parser.double('1e999'),{value:0,ok:false});
  assert.deepEqual(parser.double('1e-999'),{value:0,ok:false});
  assert.throws(()=>parser.integer(1),/string/);
});
test('numeric formatting rounds binary ties to even and recognizes spaces',()=>{
  assert.equal(formatFixedOne(1.25),'1.2');
  assert.equal(formatFixedOne(1.75),'1.8');
  assert.equal(formatFixedOne(-1.25),'-1.2');
  assert.equal(formatFixedOne(Infinity),'inf');
  assert.equal(formatFixedOne(NaN),'nan');
  assert.equal(isNumericSpace(32),true);
  assert.equal(isNumericSpace(65),false);
});
