import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {createNumericText,formatFixedOne,isNumericSpace} from '../numeric-text.mjs';

const rows=name=>gunzipSync(readFileSync(new URL('../../test/fixtures/'+name+'.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [context]=rows('qt-locale-reference');
const locale=Object.fromEntries(['decimal','group','zero','negative','exponent'].map(name=>
  [name,String.fromCharCode(context.locales.default[name])]));
locale.positive='+';
const numeric=createNumericText(locale);
const bits=value=>{const bytes=Buffer.alloc(8);bytes.writeDoubleLE(value);return bytes.toString('hex');};

// Native ABI/wire correctness belongs to the recorder audit. These unit tests
// own the independent parser's outputs; only input text/context is supplied.
for(const reference of rows('qt-numeric-reference'))test('native numeric text: '+JSON.stringify(reference.text),()=>{
  assert.deepEqual(numeric.integer(reference.text),{value:reference.integer,ok:reference.integerOk});
  const floating=numeric.double(reference.text),percent=numeric.percent(reference.text);
  assert.equal(floating.ok,reference.doubleOk);assert.equal(bits(floating.value),reference.doubleBitsLE);
  assert.equal(percent.ok,reference.percentOk);assert.equal(bits(percent.value),reference.percentScaleBitsLE);
  assert.equal(formatFixedOne(floating.value),reference.fixed1);
  assert.equal(formatFixedOne(percent.value*100),reference.percentFixed1);
});

test('numeric whitespace matches every native UTF-16 classification',()=>{
  const actual=[];
  for(let code=0;code<65536;code++)if(isNumericSpace(code))actual.push(code);
  assert.deepEqual(actual,context.spaceCodeUnits);
});

test('numeric parser owns session symbols and returned values',()=>{
  const symbols={...locale},parser=createNumericText(symbols);
  symbols.decimal='!';symbols.group='?';
  const parsed=parser.double('1,5');assert.deepEqual(parsed,{value:1.5,ok:true});parsed.value=99;
  assert.deepEqual(parser.double('1,5'),{value:1.5,ok:true});
  assert.deepEqual(parser.double('1.5'),{value:1.5,ok:true});
});
