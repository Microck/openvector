//! Port of `src/arctangent.mjs`. FPATAN retains an extended significand until
//! the following subtraction. Evaluate with guard bits, round that register
//! value, then round the stored angle and delta separately as the original
//! instructions do.
use crate::extended_arithmetic::{binary_double, binary_parts, fixed_double_at, round_significand, unit, PRECISION};
use num_bigint::BigInt;
use num_traits::Zero;
use std::sync::LazyLock;

fn series(tangent: &BigInt, working_unit: &BigInt) -> BigInt {
    let square = tangent * tangent / working_unit;
    let mut power = tangent.clone();
    let mut sum = tangent.clone();
    let mut subtract = true;
    let mut denominator = BigInt::from(3);
    loop {
        power = &power * &square / working_unit;
        let term = &power / &denominator;
        if term.is_zero() { return sum; }
        if subtract { sum -= term } else { sum += term }
        subtract = !subtract;
        denominator += 2;
    }
}

struct Constants { pi: BigInt, half_pi: BigInt, half_angle: BigInt }

static CONSTANTS: LazyLock<Constants> = LazyLock::new(|| {
    let unit = unit();
    let pi = series(&(&unit / 5), &unit) * 16 - series(&(&unit / 239), &unit) * 4;
    let half_pi = &pi / 2;
    let half_angle = series(&(&unit / 2), &unit);
    Constants { pi, half_pi, half_angle }
});

/// Angle and the following double subtraction from the previous angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AngleStep { pub angle: f64, pub difference: f64 }

/// Original extended angle register and following double subtraction.
pub fn contour_angle_step(y: f64, x: f64, previous_angle: f64) -> AngleStep {
    let c = &*CONSTANTS;
    if x == 0.0 || y == 0.0 {
        // Axis directions: atan2 is exact (0, +-pi/2, +-pi) on every libm.
        let angle = y.atan2(x);
        if angle == 0.0 { return AngleStep { angle, difference: angle - previous_angle }; }
        let quarter = if angle == std::f64::consts::PI || angle == -std::f64::consts::PI { &c.pi } else { &c.half_pi };
        let register = round_significand(&if angle < 0.0 { -quarter.clone() } else { quarter.clone() }, 64);
        let difference = binary_double(&round_significand(&(register - fixed_double_at(previous_angle, PRECISION)), 53), -(PRECISION as i64));
        return AngleStep { angle, difference };
    }
    let (absolute_x, absolute_y) = (x.abs(), y.abs());
    let transpose = absolute_y > absolute_x;
    let (smaller, larger) = if transpose { (absolute_x, absolute_y) } else { (absolute_y, absolute_x) };
    let approximate_ratio = smaller / larger;
    let numerator = binary_parts(smaller);
    let denominator = binary_parts(larger);
    // Near zero, preserve relative precision even for subnormal directions.
    // Near a quarter turn, 192 absolute guard bits already cover the register.
    let extra: u32 = if approximate_ratio < 2f64.powi(-64) && !transpose && x > 0.0 {
        (denominator.1 - numerator.1).max(0) as u32
    } else { 0 };
    let working_precision = PRECISION + extra;
    let working_unit = unit() << extra;
    let shift = working_precision as i64 + (numerator.1 - denominator.1) as i64;
    let significand = BigInt::from(numerator.0);
    let shifted = if shift >= 0 { significand << shift as u64 } else { significand >> (-shift) as u64 };
    let ratio = shifted / BigInt::from(denominator.0);
    let mut angle = if ratio > &working_unit / 4 {
        let reduced = (&ratio - &working_unit / 2) * &working_unit / (&working_unit + &ratio / 2);
        (&c.half_angle << extra) + series(&reduced, &working_unit)
    } else { series(&ratio, &working_unit) };
    if transpose { angle = (&c.half_pi << extra) - angle; }
    if x < 0.0 { angle = (&c.pi << extra) - angle; }
    let register = round_significand(&if y < 0.0 { -angle } else { angle }, 64);
    let difference = round_significand(&(&register - fixed_double_at(previous_angle, working_precision)), 53);
    AngleStep {
        angle: binary_double(&register, -(working_precision as i64)),
        difference: binary_double(&difference, -(working_precision as i64)),
    }
}
