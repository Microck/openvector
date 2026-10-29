import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/cpp-fixed-reference.json' with { type:'json' };
import { fixedDecimal,streamGeneralDecimal } from '../src/decimal-format.mjs';
import three from './fixtures/cpp-fixed-three-reference.json' with { type:'json' };
import sixteen from './fixtures/cpp-fixed-sixteen-reference.json' with { type:'json' };
import special from './fixtures/cpp-special-reference.json' with { type:'json' };

test('general key formatting retains integer zeroes and trims only fractional zeroes',() => {
  for (const [value,expected] of [[0,'0'],[-0,'0'],[10,'10'],[100000,'100000'],[-100000,'-100000'],
    [1000000,'1e+006'],[0.00001,'1e-005'],[0.0001,'0.0001'],[12.5,'12.5']])
    assert.equal(streamGeneralDecimal(value),expected);
});

test('fixed decimal output matches all 16406 original MSVCR71 strings', () => {
  assert.equal(reference.cases.length, 16406);
  for (const [index, fixture] of reference.cases.entries()) {
    assert.equal(fixedDecimal(fixture.value, 2), fixture.expected, `original fixed decimal ${index}`);
  }
});

test('three-decimal output matches all 28956 original RGB, tie and exponent probes', () => {
  assert.equal(three.cases.length, 28956);
  for (const [index, fixture] of three.cases.entries()) {
    assert.equal(fixedDecimal(fixture.value, 3), fixture.expected, `original three-decimal ${index}`);
  }
});

test('sixteen-decimal output matches all 28700 original finite and tie-adjacent probes', () => {
  assert.equal(sixteen.cases.length, 28700);
  for (const [index, fixture] of sixteen.cases.entries()) {
    assert.equal(fixedDecimal(fixture.value, 16), fixture.expected, `original sixteen-decimal ${index}`);
  }
});

test('special decimal labels match original infinity, NaN and x87 indefinite bit patterns', () => {
  assert.equal(special.cases.length, 18);
  const bits = new DataView(new ArrayBuffer(8));
  for (const fixture of special.cases) {
    bits.setBigUint64(0, BigInt('0x' + fixture.bits));
    assert.equal(fixedDecimal(bits.getFloat64(0), fixture.precision), fixture.expected);
  }
});
