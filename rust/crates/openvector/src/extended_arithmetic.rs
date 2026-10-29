//! Port of `src/extended-arithmetic.mjs`: shared numeric support for the
//! original x87 register/storage boundaries. Fixed-point values are scaled by
//! `unit() = 2^PRECISION` and held in arbitrary-precision integers, exactly as
//! the JavaScript reference uses BigInt.
use num_bigint::BigInt;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::sync::LazyLock;

pub const PRECISION: u32 = 192;

pub fn unit() -> BigInt { BigInt::one() << PRECISION }

/// MSVC converts through FISTP int64 and then loads the low int32 word.
/// Out-of-int64 conversion produces the indefinite value, whose low word is 0.
pub fn coordinate_int32(value: f64) -> i32 {
    if !(value >= -9223372036854775808.0 && value < 9223372036854775808.0) { return 0; }
    (value.trunc() as i64) as i32
}

/// Integer significand and binary exponent of a finite double (subnormals
/// use exponent -1074, matching the reference).
pub fn binary_parts(value: f64) -> (u64, i32) {
    let encoded = value.to_bits();
    let exponent = ((encoded >> 52) & 0x7ff) as i32;
    let significand = (encoded & ((1u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1u64 << 52 };
    (significand, if exponent == 0 { -1074 } else { exponent - 1075 })
}

pub fn fixed_double(value: f64) -> BigInt { fixed_double_at(value, PRECISION) }

pub fn fixed_double_at(value: f64, working_precision: u32) -> BigInt {
    if value == 0.0 { return BigInt::zero(); }
    let (significand, exponent) = binary_parts(value.abs());
    let shift = working_precision as i64 + exponent as i64;
    let significand = BigInt::from(significand);
    let fixed = if shift >= 0 { significand << shift as u64 } else { significand >> (-shift) as u64 };
    if value < 0.0 { -fixed } else { fixed }
}

fn bit_length(value: &BigInt) -> u64 { value.magnitude().bits() }

/// Round to `digits` significant bits, ties to even.
pub fn round_significand(value: &BigInt, digits: u64) -> BigInt {
    let negative = value.is_negative();
    let mut magnitude = value.abs();
    let length = bit_length(&magnitude);
    if length > digits {
        let shift = length - digits;
        let mut significand: BigInt = &magnitude >> shift;
        let remainder = &magnitude - (&significand << shift);
        let half = BigInt::one() << (shift - 1);
        if remainder > half || (remainder == half && (&significand & BigInt::one()).is_one()) { significand += 1; }
        magnitude = significand << shift;
    }
    if negative { -magnitude } else { magnitude }
}

/// Convert `value * 2^exponent` to the nearest double, including subnormals.
pub fn binary_double(value: &BigInt, exponent: i64) -> f64 {
    if value.is_zero() { return 0.0; }
    let negative = value.is_negative();
    let mut magnitude = value.abs();
    let highest = bit_length(&magnitude) as i64 - 1 + exponent;
    if highest < -1022 {
        let shift = -1074 - exponent;
        if shift > 0 {
            let distance = shift as u64;
            let mut units: BigInt = &magnitude >> distance;
            let remainder = &magnitude - (&units << distance);
            let half = BigInt::one() << (distance - 1);
            if remainder > half || (remainder == half && (&units & BigInt::one()).is_one()) { units += 1; }
            magnitude = units;
        } else { magnitude <<= (-shift) as u64; }
        let rounded = magnitude.to_f64().unwrap() * f64::from_bits(1);
        return if negative { -rounded } else { rounded };
    }
    magnitude = round_significand(&magnitude, 53);
    let shift = (bit_length(&magnitude) as i64 - 53).max(0);
    let mantissa = (&magnitude >> shift as u64).to_f64().unwrap();
    let rounded = mantissa * pow2(exponent + shift);
    if negative { -rounded } else { rounded }
}

/// Exact 2^n for any n where the product stays representable, matching JS `2 ** n`.
fn pow2(n: i64) -> f64 {
    if n > 1023 { return f64::INFINITY; }
    if n >= -1022 { return f64::from_bits(((n + 1023) as u64) << 52); }
    if n >= -1074 { return f64::from_bits(1u64 << (n + 1074)); }
    0.0
}

fn logarithm_series(argument: &BigInt) -> BigInt {
    let unit = unit();
    let square = argument * argument / &unit;
    let mut power = argument.clone();
    let mut sum = argument.clone();
    let mut denominator = BigInt::from(3);
    loop {
        power = &power * &square / &unit;
        let term = &power / &denominator;
        if term.is_zero() { return sum * 2; }
        sum += term;
        denominator += 2;
    }
}

struct Constants { ln2: BigInt, native_ln2: BigInt, native_log2e: BigInt }

static CONSTANTS: LazyLock<Constants> = LazyLock::new(|| {
    let unit = unit();
    let ln2 = logarithm_series(&(&unit / 3));
    let native_ln2 = round_significand(&ln2, 64);
    let native_log2e = round_significand(&(&unit * &unit / &ln2), 64);
    Constants { ln2, native_ln2, native_log2e }
});

fn constants() -> &'static Constants { &CONSTANTS }

fn fixed_logarithm(value: f64) -> BigInt {
    let unit = unit();
    let (significand, exponent) = binary_parts(value);
    let significand = BigInt::from(significand);
    let highest = bit_length(&significand) as i64 - 1;
    let normalized = significand << (PRECISION as i64 - highest) as u64;
    let argument = (&normalized - &unit) * &unit / (&normalized + &unit);
    logarithm_series(&argument) + BigInt::from(exponent as i64 + highest) * &constants().ln2
}

fn nearest_integer(value: &BigInt) -> BigInt {
    let unit = unit();
    let negative = value.is_negative();
    let magnitude = value.abs();
    let mut integral: BigInt = &magnitude / &unit;
    let remainder = &magnitude % &unit;
    let twice = &remainder * 2;
    if twice > unit || (twice == unit && (&integral & BigInt::one()).is_one()) { integral += 1; }
    if negative { -integral } else { integral }
}

fn exponential_minus_one(argument: &BigInt) -> BigInt {
    let unit = unit();
    let mut term = argument.clone();
    let mut sum = argument.clone();
    let mut denominator = BigInt::from(2);
    loop {
        term = &term * argument / (&unit * &denominator);
        if term.is_zero() { return sum; }
        sum += &term;
        denominator += 1;
    }
}

/// 0x49f8d5..0x49f90b: retain extended transcendental results until the
/// original 53-bit arithmetic/store boundaries, including FLDLN2/FLDL2E.
pub fn fitting_threshold_multiplier(initial: f64, final_: f64, iterations: f64, fraction: f64) -> f64 {
    let unit = unit();
    let c = constants();
    let ratio = final_ / initial;
    let logarithm = round_significand(&((fixed_logarithm(ratio) * &unit / &c.ln2) * &c.native_ln2 / &unit), 64);
    let divided = round_significand(&(&logarithm * &unit / fixed_double(iterations * fraction)), 53);
    let exponent = round_significand(&(&divided * &c.native_log2e / &unit), 53);
    let integral = nearest_integer(&exponent);
    let remainder = round_significand(&(&exponent - &integral * &unit), 53);
    let transcendental = round_significand(&exponential_minus_one(&(&remainder * &c.ln2 / &unit)), 64);
    let scaled = round_significand(&(&unit + transcendental), 53);
    binary_double(&scaled, integral.to_i64().unwrap() - PRECISION as i64)
}
