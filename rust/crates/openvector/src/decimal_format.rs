//! Port of `src/decimal-format.mjs`: the original MSVCR71/MSVCP71 decimal
//! formatting, including its 80-bit intermediate digit generation.
use crate::extended_arithmetic::binary_parts;
use num_bigint::BigInt;
use num_traits::{One, ToPrimitive, Zero};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Exact decimal expansion of a finite positive double: digits and the power
/// of ten of the first digit. JavaScript `toFixed`/`toExponential` round this
/// exact value half away from zero ("pick the larger n"), which `format!`
/// does not (it rounds ties to even), so the reference rounding is rebuilt.
fn exact_digits(value: f64) -> (Vec<u8>, i64) {
    let (significand, exponent) = binary_parts(value);
    let (numerator, scale) = if exponent >= 0 {
        (BigInt::from(significand) << exponent as u64, 0i64)
    } else {
        // sig * 2^e = sig * 5^-e / 10^-e
        (BigInt::from(significand) * BigInt::from(5).pow((-exponent) as u32), -(exponent as i64))
    };
    let text = numerator.to_string();
    let digits: Vec<u8> = text.bytes().map(|b| b - b'0').collect();
    let first_power = digits.len() as i64 - 1 - scale;
    (digits, first_power)
}

/// Round exact digits to keep `keep` digits (half up). Returns digits and
/// the possibly incremented leading power.
fn round_half_up(mut digits: Vec<u8>, mut first_power: i64, keep: i64) -> (Vec<u8>, i64) {
    if keep < 0 { return (vec![], first_power); }
    let keep = keep as usize;
    if digits.len() <= keep {
        digits.resize(keep, 0);
        return (digits, first_power);
    }
    let up = digits[keep] >= 5;
    digits.truncate(keep);
    if up {
        let mut index = keep;
        loop {
            if index == 0 { digits.insert(0, 1); first_power += 1; digits.truncate(keep.max(1)); break; }
            index -= 1;
            if digits[index] == 9 { digits[index] = 0; } else { digits[index] += 1; break; }
        }
    }
    (digits, first_power)
}

fn js_to_exponential5(value: f64) -> (String, i64) {
    let (digits, power) = exact_digits(value.abs());
    let (digits, power) = round_half_up(digits, power, 6);
    let text: String = digits.iter().map(|d| (b'0' + d) as char).collect();
    let sign = if value < 0.0 { "-" } else { "" };
    (format!("{sign}{}.{}", &text[..1], &text[1..]), power)
}

fn js_to_fixed(value: f64, places: i64) -> String {
    let negative = value < 0.0;
    let (digits, power) = exact_digits(value.abs());
    // Keep all digits down to 10^-places.
    let keep = power + 1 + places;
    let (digits, power) = if keep <= 0 {
        // Value below the rounding position: rounds to 0 or one unit.
        if keep == 0 && digits[0] >= 5 { (vec![1], -places) } else { (vec![], power) }
    } else { round_half_up(digits, power, keep) };
    // Build integer of scaled value.
    let mut scaled = String::new();
    if digits.is_empty() { scaled.push('0'); } else {
        for d in &digits { scaled.push((b'0' + d) as char); }
        let trailing = power - (digits.len() as i64 - 1) + places;
        for _ in 0..trailing.max(0) { scaled.push('0'); }
    }
    let places = places as usize;
    let padded = format!("{:0>width$}", scaled, width = places + 1);
    let (integer, fraction) = padded.split_at(padded.len() - places);
    let body = if places == 0 { integer.to_string() } else { format!("{integer}.{fraction}") };
    // JavaScript keeps the sign even when the value rounds to zero.
    if negative { format!("-{body}") } else { body }
}

fn trim_fraction_zeros(text: &str) -> String {
    if !text.contains('.') { return text.to_string(); }
    let trimmed = text.trim_end_matches('0');
    trimmed.trim_end_matches('.').to_string()
}

/// Native default streams use six significant digits and three exponent digits.
/// Document cache keys and tool history must use the same formatter.
pub fn stream_general_decimal(value: f64) -> String {
    if value == 0.0 { return "0".into(); }
    let (mantissa, exponent) = js_to_exponential5(value);
    if exponent < -4 || exponent >= 6 {
        return format!("{}e{}{:03}", trim_fraction_zeros(&mantissa), if exponent < 0 { '-' } else { '+' }, exponent.abs());
    }
    let fixed = js_to_fixed(value, (5 - exponent).max(0));
    if fixed.contains('.') { trim_fraction_zeros(&fixed) } else { fixed }
}

#[derive(Clone)]
struct Long { mantissa: BigInt, exponent: i64 }

static DECIMAL_POWERS: LazyLock<Mutex<HashMap<i64, Long>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn bits(value: &BigInt) -> i64 { value.magnitude().bits() as i64 }

// The runtime power table's extra rounded word is undone before use. Its
// effective entries are nearest-even 80-bit significands of exact powers.
fn decimal_power(power: i64) -> Long {
    if let Some(found) = DECIMAL_POWERS.lock().unwrap().get(&power) { return found.clone(); }
    let ten = BigInt::from(10);
    let mut numerator = if power > 0 { ten.pow(power as u32) } else { BigInt::one() };
    let mut denominator = if power > 0 { BigInt::one() } else { ten.pow((-power) as u32) };
    let mut exponent = bits(&numerator) - bits(&denominator);
    let below = if exponent >= 0 { numerator < (&denominator << exponent as u64) } else { (&numerator << (-exponent) as u64) < denominator };
    if below { exponent -= 1; }
    let shift = 79 - exponent;
    if shift >= 0 { numerator <<= shift as u64; } else { denominator <<= (-shift) as u64; }
    let mut mantissa = &numerator / &denominator;
    let remainder = &numerator % &denominator;
    let twice = remainder * 2;
    if twice > denominator || (twice == denominator && (&mantissa & BigInt::one()).is_one()) { mantissa += 1; }
    let factor = Long { mantissa, exponent: exponent + 16383 };
    DECIMAL_POWERS.lock().unwrap().insert(power, factor.clone());
    factor
}

// 0x7c3730ed retains only product diagonals at or above the fourth 16-bit
// word. Carries from the omitted low diagonals never enter its accumulator.
fn multiply_long(left: &Long, right: &Long) -> Long {
    let mask = BigInt::from(0xffffu32);
    let mut product = BigInt::zero();
    for a in 0..5u64 {
        let limb = (&left.mantissa >> (a * 16)) & &mask;
        for b in (4 - a)..5u64 {
            product += (&limb * ((&right.mantissa >> (b * 16)) & &mask)) << ((a + b - 4) * 16);
        }
    }
    let mut exponent = left.exponent + right.exponent - 16382;
    let limit = BigInt::one() << 95u32;
    while product < limit { product <<= 1u32; exponent -= 1; }
    let mut mantissa: BigInt = &product >> 16u32;
    let remainder = &product & &mask;
    let half = BigInt::from(0x8000u32);
    if remainder > half || (remainder == half && (&mantissa & BigInt::one()).is_one()) { mantissa += 1; }
    if mantissa == BigInt::one() << 80u32 { mantissa >>= 1u32; exponent += 1; }
    Long { mantissa, exponent }
}

struct Digits { digits: String, point: i64 }

fn decimal_digits(value: f64) -> Digits {
    let (significand, exponent) = binary_parts(value);
    let significand = BigInt::from(significand);
    let highest = bits(&significand) - 1;
    let mut number = Long { mantissa: significand << (79 - highest) as u64, exponent: exponent as i64 + highest + 16383 };
    let top = (&number.mantissa >> 72u32).to_i64().unwrap();
    let mixed = ((((number.exponent >> 8) + top * 2) * 77 - 0x134312f4 + number.exponent * 0x4d10) as i64) as i32;
    let mut point = (mixed >> 16) as i64;
    let mut power = point.abs();
    let mut step: i64 = if point > 0 { -1 } else { 1 };
    while power != 0 {
        let digit = power & 7;
        if digit != 0 { number = multiply_long(&number, &decimal_power(digit * step)); }
        power >>= 3;
        step *= 8;
    }
    if number.exponent > 0x3ffe {
        point += 1;
        number = multiply_long(&number, &Long { mantissa: BigInt::parse_bytes(b"cccccccccccccccccccc", 16).unwrap(), exponent: 0x3ffb });
    }
    let mut fraction: BigInt = &number.mantissa << (8 + number.exponent - 0x3ffe) as u64;
    let fraction_mask = (BigInt::one() << 88u32) - 1;
    let mut digits = String::new();
    for _ in 0..18 {
        fraction *= 10;
        digits.push_str(&(&fraction >> 88u32).to_string());
        fraction &= &fraction_mask;
    }
    let mut rounded: BigInt = digits[..17].parse().unwrap();
    if digits.as_bytes()[17] >= b'5' { rounded += 1; }
    let mut text = format!("{:0>17}", rounded.to_string());
    if text.len() > 17 { point += 1; text.truncate(17); }
    Digits { digits: text, point }
}

fn fixed_field(d: &Digits, precision: i64, negative: bool) -> String {
    let digits = d.digits.as_bytes();
    let needed = d.point + precision;
    let mut scaled = BigInt::zero();
    if needed == 0 {
        if digits[0] >= b'5' { scaled = BigInt::one(); }
    } else if needed > 0 {
        let n = needed as usize;
        let text = if n >= digits.len() { format!("{}{}", d.digits, "0".repeat(n - digits.len())) } else { d.digits[..n].to_string() };
        scaled = text.parse().unwrap();
        if n < digits.len() && digits[n] >= b'5' { scaled += 1; }
    }
    let p = precision as usize;
    let field = format!("{:0>width$}", scaled.to_string(), width = p + 1);
    let (integer, fraction) = field.split_at(field.len() - p);
    format!("{}{integer}.{fraction}", if negative { "-" } else { "" })
}

/// Original MSVCR71 sprintf fixed precision, including its intermediate
/// 17-digit conversion and second decimal rounding. Signed zero is normalized.
pub fn fixed_decimal(value: f64, precision: usize) -> String {
    if !value.is_finite() {
        // The CRT distinguishes x87 indefinite from other NaNs by their bits,
        // then rounds truncated labels as though the letters were decimal digits.
        let encoded = value.to_bits();
        let mantissa = encoded & ((1u64 << 52) - 1);
        let label = if mantissa == 0 { "#INF" } else if encoded == 0xfff8000000000000 { "#IND" }
            else if mantissa & (1u64 << 51) != 0 { "#QNAN" } else { "#SNAN" };
        let mut fraction: Vec<u8> = label.bytes().chain(std::iter::repeat(b'0')).take(precision.max(label.len())).collect();
        fraction.truncate(precision);
        if precision < label.len() && label.as_bytes()[precision] >= b'5' {
            if let Some(last) = fraction.last_mut() { *last += 1; }
        }
        let sign = if encoded >> 63 != 0 { "-" } else { "" };
        return format!("{sign}1.{}", String::from_utf8(fraction).unwrap());
    }
    let digits = if value == 0.0 { Digits { digits: "0".into(), point: 0 } } else { decimal_digits(value.abs()) };
    fixed_field(&digits, precision as i64, value < 0.0)
}

/// Initial EPS streams use general precision three until a coordinate sets
/// the fixed flag. RGB is bounded to [0,1], so no exponent notation is used.
pub fn general_rgb_decimal(value: f64) -> String {
    if value == 0.0 { return "0".into(); }
    let digits = decimal_digits(value);
    let field = fixed_field(&digits, 3 - digits.point, false);
    field.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// MSVCP71 fixed streams scale large magnitudes by repeated rounded 1e-10
/// multiplication, then reinsert zeros before the decimal point.
pub fn stream_fixed_decimal(value: f64, precision: usize) -> String {
    let negative = value < 0.0;
    let mut magnitude = if negative { -value } else { value };
    let mut zeros = 0;
    while magnitude >= 1e35 && zeros < 5000 { magnitude *= 1e-10; zeros += 10; }
    let mut field = fixed_decimal(magnitude, precision);
    if zeros > 0 {
        let (integer, fraction) = field.split_once('.').unwrap();
        field = format!("{integer}{}.{fraction}", "0".repeat(zeros));
    }
    format!("{}{field}", if negative { "-" } else { "" })
}
