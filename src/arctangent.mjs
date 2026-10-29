// FPATAN retains an extended significand until the following subtraction.
// Evaluate with guard bits, round that register value, then round the stored
// angle and delta separately as the original instructions do.
import { precision, unit, binaryParts, fixedDouble, roundSignificand, binaryDouble } from './extended-arithmetic.mjs';

function series(tangent, workingUnit = unit) {
  const square = tangent * tangent / workingUnit;
  let power = tangent, sum = tangent, subtract = true;
  for (let denominator = 3n; ; denominator += 2n) {
    power = power * square / workingUnit;
    const term = power / denominator;
    if (term === 0n) return sum;
    sum += subtract ? -term : term;
    subtract = !subtract;
  }
}

const pi = 16n * series(unit / 5n) - 4n * series(unit / 239n);
const halfPi = pi / 2n;
const halfAngle = series(unit / 2n);

/** Original extended angle register and following double subtraction. */
export function contourAngleStep(y, x, previousAngle) {
  if (x === 0 || y === 0) {
    const angle = Math.atan2(y, x);
    if (angle === 0) return { angle, difference:angle - previousAngle };
    const quarter = angle === Math.PI || angle === -Math.PI ? pi : halfPi;
    const register = roundSignificand(angle < 0 ? -quarter : quarter, 64);
    return { angle, difference:binaryDouble(roundSignificand(register - fixedDouble(previousAngle), 53), -Number(precision)) };
  }
  const absoluteX = Math.abs(x), absoluteY = Math.abs(y);
  const transpose = absoluteY > absoluteX;
  const smaller = transpose ? absoluteX : absoluteY, larger = transpose ? absoluteY : absoluteX;
  const approximateRatio = smaller / larger;
  const numerator = binaryParts(smaller), denominator = binaryParts(larger);
  // Near zero, preserve relative precision even for subnormal directions.
  // Near a quarter turn, 192 absolute guard bits already cover the register.
  const extra = approximateRatio < 2 ** -64 && !transpose && x > 0 ? BigInt(Math.max(0, denominator.exponent - numerator.exponent)) : 0n;
  const workingPrecision = precision + extra, workingUnit = unit << extra;
  const shift = workingPrecision + BigInt(numerator.exponent - denominator.exponent);
  const shifted = shift >= 0n ? numerator.significand << shift : numerator.significand >> -shift;
  const ratio = shifted / denominator.significand;
  let angle;
  if (ratio > workingUnit / 4n) {
    const reduced = (ratio - workingUnit / 2n) * workingUnit / (workingUnit + ratio / 2n);
    angle = (halfAngle << extra) + series(reduced, workingUnit);
  } else angle = series(ratio, workingUnit);
  if (transpose) angle = (halfPi << extra) - angle;
  if (x < 0) angle = (pi << extra) - angle;
  const register = roundSignificand(y < 0 ? -angle : angle, 64);
  const difference = roundSignificand(register - fixedDouble(previousAngle, workingPrecision), 53);
  return { angle:binaryDouble(register, -Number(workingPrecision)), difference:binaryDouble(difference, -Number(workingPrecision)) };
}
