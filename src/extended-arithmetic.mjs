// Shared numeric support for original x87 register/storage boundaries.
export const precision = 192n;
export const unit = 1n << precision;
const mantissaMask = (1n << 52n) - 1n;
const bits = new DataView(new ArrayBuffer(8));

// MSVC converts through FISTP int64 and then loads the low int32 word.
// Out-of-int64 conversion produces the indefinite value, whose low word is 0.
export function coordinateInt32(value) {
  if (value < -9223372036854775808 || value >= 9223372036854775808) return 0;
  return Number(BigInt.asIntN(32,BigInt(Math.trunc(value))));
}

export function binaryParts(value) {
  bits.setFloat64(0, value);
  const encoded = bits.getBigUint64(0);
  const exponent = Number(encoded >> 52n & 0x7ffn);
  const significand = (encoded & mantissaMask) | (exponent === 0 ? 0n : 1n << 52n);
  return { significand, exponent:exponent === 0 ? -1074 : exponent - 1075 };
}

export function fixedDouble(value, workingPrecision = precision) {
  if (value === 0) return 0n;
  const { significand, exponent } = binaryParts(Math.abs(value));
  const shift = workingPrecision + BigInt(exponent);
  const fixed = shift >= 0n ? significand << shift : significand >> -shift;
  return value < 0 ? -fixed : fixed;
}

export function roundSignificand(value, digits) {
  const negative = value < 0n;
  if (negative) value = -value;
  const shift = BigInt(value.toString(2).length) - BigInt(digits);
  if (shift > 0n) {
    let significand = value >> shift;
    const remainder = value - (significand << shift), half = 1n << (shift - 1n);
    if (remainder > half || (remainder === half && (significand & 1n))) significand++;
    value = significand << shift;
  }
  return negative ? -value : value;
}

export function binaryDouble(value, exponent) {
  if (value === 0n) return 0;
  const negative = value < 0n;
  if (negative) value = -value;
  const highest = value.toString(2).length - 1 + exponent;
  if (highest < -1022) {
    const shift = -1074 - exponent;
    if (shift > 0) {
      const distance = BigInt(shift);
      let units = value >> distance;
      const remainder = value - (units << distance), half = 1n << (distance - 1n);
      if (remainder > half || (remainder === half && (units & 1n))) units++;
      value = units;
    } else value <<= BigInt(-shift);
    const rounded = Number(value) * Number.MIN_VALUE;
    return negative ? -rounded : rounded;
  }
  value = roundSignificand(value, 53);
  const shift = Math.max(0, value.toString(2).length - 53);
  const rounded = Number(value >> BigInt(shift)) * 2 ** (exponent + shift);
  return negative ? -rounded : rounded;
}

function logarithmSeries(argument) {
  const square = argument * argument / unit;
  let power = argument, sum = argument;
  for (let denominator = 3n; ; denominator += 2n) {
    power = power * square / unit;
    const term = power / denominator;
    if (term === 0n) return sum * 2n;
    sum += term;
  }
}

const ln2 = logarithmSeries(unit / 3n);
const nativeLn2 = roundSignificand(ln2, 64);
const nativeLog2E = roundSignificand(unit * unit / ln2, 64);

function fixedLogarithm(value) {
  const { significand, exponent } = binaryParts(value);
  const highest = significand.toString(2).length - 1;
  const normalized = significand << (precision - BigInt(highest));
  const argument = (normalized - unit) * unit / (normalized + unit);
  return logarithmSeries(argument) + BigInt(exponent + highest) * ln2;
}

function nearestInteger(value) {
  const negative = value < 0n;
  if (negative) value = -value;
  let integral = value / unit;
  const remainder = value % unit;
  if (remainder * 2n > unit || (remainder * 2n === unit && (integral & 1n))) integral++;
  return negative ? -integral : integral;
}

function exponentialMinusOne(argument) {
  let term = argument, sum = argument;
  for (let denominator = 2n; ; denominator++) {
    term = term * argument / (unit * denominator);
    if (term === 0n) return sum;
    sum += term;
  }
}

/** 0x49f8d5..0x49f90b: retain extended transcendental results until the
 * original 53-bit arithmetic/store boundaries, including FLDLN2/FLDL2E. */
export function fittingThresholdMultiplier(initial, final, iterations, fraction) {
  const ratio = final / initial;
  const logarithm = roundSignificand((fixedLogarithm(ratio) * unit / ln2) * nativeLn2 / unit, 64);
  const divided = roundSignificand(logarithm * unit / fixedDouble(iterations * fraction), 53);
  const exponent = roundSignificand(divided * nativeLog2E / unit, 53);
  const integral = nearestInteger(exponent);
  const remainder = roundSignificand(exponent - integral * unit, 53);
  const transcendental = roundSignificand(exponentialMinusOne(remainder * ln2 / unit), 64);
  const scaled = roundSignificand(unit + transcendental, 53);
  return binaryDouble(scaled, Number(integral - precision));
}

