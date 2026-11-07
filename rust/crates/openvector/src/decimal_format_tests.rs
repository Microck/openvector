//! Arbitrary-precision oracle frozen from the pre-optimization formatter.
//! Keep this arithmetic independent of the bounded production implementation.
use super::{binary_parts, BigInt, One, ToPrimitive, Zero};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

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


fn next(random: &mut u64) -> u64 {
    *random ^= *random << 13;
    *random ^= *random >> 7;
    *random ^= *random << 17;
    *random
}

#[test]
fn bounded_long_product_matches_arbitrary_precision_omitted_diagonals() {
    let mut random = 0x86d516e50b04ab1du64;
    let patterns = [1u128 << 79, (1u128 << 79) + 1, (1u128 << 80) - 1,
        0x8000_0000_0000_ffff_ffff, 0xffff_ffff_ffff_0000_0000,
        0xaaaa_aaaa_aaaa_aaaa_aaaa, 0xcccc_cccc_cccc_cccc_cccc];
    let compare = |left: u128, right: u128| {
        let expected = multiply_long(&Long { mantissa: left.into(), exponent: 16383 },
            &Long { mantissa: right.into(), exponent: 16380 });
        let actual = super::multiply_long(&super::Long { mantissa: left, exponent: 16383 },
            &super::Long { mantissa: right, exponent: 16380 });
        assert_eq!(actual.mantissa, expected.mantissa.to_u128().unwrap(), "{left:x} * {right:x}");
        assert_eq!(actual.exponent, expected.exponent, "{left:x} * {right:x}");
    };
    for &left in &patterns { for &right in &patterns { compare(left, right); } }
    for _ in 0..10_000 {
        let mut mantissa = || (1u128 << 79) | (((next(&mut random) as u128) << 64 | next(&mut random) as u128) & ((1u128 << 79) - 1));
        compare(mantissa(), mantissa());
    }
}

#[test]
fn decimal_digits_match_arbitrary_precision_across_all_binary_exponents() {
    let compare = |bits: u64| {
        let value = f64::from_bits(bits);
        if value == 0.0 || !value.is_finite() { return; }
        let expected = decimal_digits(value.abs());
        let actual = super::decimal_digits(value.abs());
        assert_eq!(actual.digits, expected.digits, "digits for {bits:016x}");
        assert_eq!(actual.point, expected.point, "point for {bits:016x}");
    };
    // Both sides of powers of two, the significand midpoint and the largest
    // finite value cover every normal binade, including under/overflow edges.
    for exponent in 1..0x7ffu64 {
        for mantissa in [0, 1, (1u64 << 51) - 1, 1u64 << 51, (1u64 << 52) - 2, (1u64 << 52) - 1] {
            compare(exponent << 52 | mantissa);
        }
    }
    for bit in 0..52 { for offset in [-1i64, 0, 1] { compare(((1u64 << bit) as i64 + offset) as u64); } }
    let mut random = 0x766563746f726d73u64;
    for _ in 0..4096 { compare(next(&mut random)); }
    // Decimal ties and decimal exponent transitions exercise the second
    // rounding boundary used by fixed and stream formatting.
    for exponent in -323..=308 {
        let value = 10.0f64.powi(exponent);
        if value != 0.0 && value.is_finite() {
            for offset in [-1i64, 0, 1] { compare((value.to_bits() as i64 + offset) as u64); }
        }
    }
}
