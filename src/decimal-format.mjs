import { binaryParts } from './extended-arithmetic.mjs';

/** Native default streams use six significant digits and three exponent digits.
 * Document cache keys and tool history must use the same formatter. */
export function streamGeneralDecimal(value) {
  if (value === 0) return '0';
  const [mantissa, exponentText] = value.toExponential(5).split('e');
  const exponent = Number(exponentText);
  if (exponent < -4 || exponent >= 6)
    return `${mantissa.replace(/\.?0+$/,'')}e${exponent < 0 ? '-' : '+'}${String(Math.abs(exponent)).padStart(3,'0')}`;
  const fixed = value.toFixed(Math.max(0,5-exponent));
  return fixed.includes('.') ? fixed.replace(/\.?0+$/,'') : fixed;
}

const wordMask = 0xffffn;
const fractionMask = (1n << 88n) - 1n;
const decimalPowers = new Map();
const specialBits = new DataView(new ArrayBuffer(8));

// The runtime power table's extra rounded word is undone before use. Its
// effective entries are nearest-even 80-bit significands of exact powers.
function decimalPower(power) {
  if (decimalPowers.has(power)) return decimalPowers.get(power);
  let numerator = power > 0 ? 10n ** BigInt(power) : 1n;
  let denominator = power > 0 ? 1n : 10n ** BigInt(-power);
  let exponent = numerator.toString(2).length - denominator.toString(2).length;
  const belowPower = exponent >= 0 ? numerator < denominator << BigInt(exponent) :
    numerator << BigInt(-exponent) < denominator;
  if (belowPower) exponent--;
  const shift = 79 - exponent;
  if (shift >= 0) numerator <<= BigInt(shift); else denominator <<= BigInt(-shift);
  let mantissa = numerator / denominator;
  const remainder = numerator % denominator;
  if (remainder * 2n > denominator || (remainder * 2n === denominator && (mantissa & 1n))) mantissa++;
  const factor = { mantissa, exponent:exponent + 16383 };
  decimalPowers.set(power, factor);
  return factor;
}

// 0x7c3730ed retains only product diagonals at or above the fourth 16-bit
// word. Carries from the omitted low diagonals never enter its accumulator.
function multiplyLong(left, right) {
  let product = 0n;
  for (let a = 0; a < 5; a++) {
    const limb = left.mantissa >> BigInt(a * 16) & wordMask;
    for (let b = 4 - a; b < 5; b++) {
      product += limb * (right.mantissa >> BigInt(b * 16) & wordMask) << BigInt((a + b - 4) * 16);
    }
  }
  let exponent = left.exponent + right.exponent - 16382;
  while (product < 1n << 95n) { product <<= 1n; exponent--; }
  let mantissa = product >> 16n;
  const remainder = product & wordMask;
  if (remainder > 0x8000n || (remainder === 0x8000n && (mantissa & 1n))) mantissa++;
  if (mantissa === 1n << 80n) { mantissa >>= 1n; exponent++; }
  return { mantissa, exponent };
}

function decimalDigits(value) {
  const parts = binaryParts(value), highest = parts.significand.toString(2).length - 1;
  let number = { mantissa:parts.significand << BigInt(79 - highest), exponent:parts.exponent + highest + 16383 };
  let point = ((((number.exponent >> 8) + Number(number.mantissa >> 72n) * 2) * 77 - 0x134312f4 + number.exponent * 0x4d10) | 0) >> 16;
  let power = Math.abs(point), powerStep = point > 0 ? -1 : 1;
  while (power !== 0) {
    const digit = power & 7;
    if (digit !== 0) number = multiplyLong(number, decimalPower(digit * powerStep));
    power >>= 3; powerStep *= 8;
  }
  if (number.exponent > 0x3ffe) {
    point++;
    number = multiplyLong(number, { mantissa:0xccccccccccccccccccccn, exponent:0x3ffb });
  }
  let fraction = number.mantissa << BigInt(8 + number.exponent - 0x3ffe);
  let digits = '';
  for (let index = 0; index < 18; index++) {
    fraction *= 10n;
    digits += (fraction >> 88n).toString();
    fraction &= fractionMask;
  }
  let rounded = BigInt(digits.slice(0, 17));
  if (digits[17] >= '5') rounded++;
  digits = rounded.toString().padStart(17, '0');
  if (digits.length > 17) { point++; digits = digits.slice(0, 17); }
  return { digits, point };
}

function fixedField({ digits, point }, precision, negative) {
  const needed = point + precision;
  let scaled = 0n;
  if (needed === 0) scaled = digits[0] >= '5' ? 1n : 0n;
  else if (needed > 0) {
    scaled = BigInt(needed >= digits.length ? digits + '0'.repeat(needed - digits.length) : digits.slice(0, needed));
    if (needed < digits.length && digits[needed] >= '5') scaled++;
  }
  const field = scaled.toString().padStart(precision + 1, '0');
  return `${negative ? '-' : ''}${field.slice(0, -precision)}.${field.slice(-precision)}`;
}

/** Original MSVCR71 sprintf fixed precision, including its intermediate
 * 17-digit conversion and second decimal rounding. Signed zero is normalized. */
export function fixedDecimal(value, precision) {
  if (!Number.isFinite(value)) {
    // The CRT distinguishes x87 indefinite from other NaNs by their bits,
    // then rounds truncated labels as though the letters were decimal digits.
    specialBits.setFloat64(0, value);
    const encoded = specialBits.getBigUint64(0), mantissa = encoded & ((1n << 52n) - 1n);
    let label;
    if (mantissa === 0n) label = '#INF';
    else if (encoded === 0xfff8000000000000n) label = '#IND';
    else label = mantissa & (1n << 51n) ? '#QNAN' : '#SNAN';
    let fraction = label.padEnd(precision, '0').slice(0, precision);
    if (precision < label.length && label[precision] >= '5') {
      fraction = fraction.slice(0, -1) + String.fromCharCode(fraction.charCodeAt(fraction.length - 1) + 1);
    }
    return `${encoded >> 63n ? '-' : ''}1.${fraction}`;
  }
  const digits = value === 0 ? { digits:'0', point:0 } : decimalDigits(Math.abs(value));
  return fixedField(digits, precision, value < 0);
}

/** Initial EPS streams use general precision three until a coordinate sets
 * the fixed flag. RGB is bounded to [0,1], so no exponent notation is used. */
export function generalRgbDecimal(value) {
  if (value === 0) return '0';
  const digits = decimalDigits(value);
  return fixedField(digits, 3 - digits.point, false).replace(/0+$/, '').replace(/\.$/, '');
}

/** MSVCP71 fixed streams scale large magnitudes by repeated rounded 1e-10
 * multiplication, then reinsert zeros before the decimal point. */
export function streamFixedDecimal(value, precision) {
  const negative = value < 0;
  let magnitude = negative ? -value : value, zeros = 0;
  while (magnitude >= 1e35 && zeros < 5000) { magnitude *= 1e-10; zeros += 10; }
  let field = fixedDecimal(magnitude, precision);
  if (zeros > 0) {
    const [integer, fraction] = field.split('.');
    field = integer + '0'.repeat(zeros) + '.' + fraction;
  }
  return (negative ? '-' : '') + field;
}
