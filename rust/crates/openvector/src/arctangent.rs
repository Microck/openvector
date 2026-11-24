//! Port of `src/arctangent.mjs`. FPATAN retains an extended significand until
//! the following subtraction. Evaluate with guard bits, round that register
//! value, then round the stored angle and delta separately as the original
//! instructions do.
use crate::extended_arithmetic::{binary_double, binary_parts, fixed_double_at, round_significand, unit, PRECISION};
use crate::fast_fixed::{FixedUint, SignedFixed};
use num_bigint::BigInt;
use num_traits::Zero;
use std::sync::LazyLock;

/// The arctangent series on the stack, for the working precision the engine
/// uses on every ordinary direction.
///
/// `series` below is the reference: it works on the tangent's magnitude, so
/// every shift is a floor and every division a truncation of a non-negative
/// value, and it applies the sign to the sum at the end. At `PRECISION` the
/// tangent is under a quarter turn (below `2^190`), so its square and every
/// power stay below `2^384`. Returns `None` when an input leaves that domain,
/// and the caller falls back to the arbitrary-precision path.
fn series_fixed_uint(magnitude: &FixedUint) -> Option<FixedUint> {
    // `PRECISION` is a whole number of limbs, so the series' floor shift is a
    // limb move rather than the general bit-shift loop. The power update is
    // `(power * square) >> 192` with both operands at most three limbs, which
    // `mul_shift_192` forms in named words; the general `mul`/`shr_limbs` pair
    // stays as the fallback for an operand outside that width.
    let shift = |value: FixedUint| value.shr_limbs(PRECISION / 64);
    let square = match magnitude.mul_shift_192(magnitude) {
        Some(product) => product,
        None => shift(magnitude.mul(magnitude)?),
    };
    let mut power = *magnitude;
    let mut sum = *magnitude;
    let mut subtract = true;
    let mut denominator = 3u64;
    // The head: the power is still wider than one limb, so the term is a
    // general truncating division and the sum's update a general pass.
    let mut limb = loop {
        power = match power.mul_shift_192(&square) {
            Some(product) => product,
            None => shift(power.mul(&square)?),
        };
        if let Some(limb) = power.single_limb() {
            break limb;
        }
        let term = power.div_u64(denominator)?;
        if term.is_zero() {
            return Some(sum);
        }
        let ok = if subtract { sum.sub_assign(&term) } else { sum.add_assign(&term) };
        if !ok { return None; }
        subtract = !subtract;
        denominator += 2;
    };
    // The tail: the power fits in one limb, and it stays there. The update is
    // `(power * square) >> 192` with `power < 2^64`, so the quotient is below
    // `2^64` whenever `square < 2^192` — the series' own domain, where the
    // tangent is under a quarter turn. The product's word above the third is
    // then a three-step carry chain over the square's limbs, the term is one
    // 64-bit divide, and the sum's update is a one-limb add or subtract: no
    // 72-byte value is built per term and no length is tested per term.
    if square.bit_len() <= 192 {
        let square_limbs = square.low_three_limbs();
        loop {
            // The head stopped on an updated power whose term it has not yet
            // taken, so the term comes first here and the update last.
            let term = limb / denominator;
            if term == 0 {
                return Some(sum);
            }
            let ok = if subtract { sum.sub_u64_assign(term) } else { sum.add_u64_assign(term) };
            if !ok { return None; }
            subtract = !subtract;
            denominator += 2;
            let wide = limb as u128;
            let mut carry = 0u128;
            for word in square_limbs {
                let product = wide * (word as u128) + carry;
                carry = product >> 64;
            }
            limb = carry as u64;
        }
    }
    // A square wider than three limbs is outside the tail's domain; the head's
    // general form finishes the series from the one-limb power it stopped at.
    power = FixedUint::from_u64(limb);
    loop {
        let term = power.div_u64(denominator)?;
        if term.is_zero() {
            return Some(sum);
        }
        let ok = if subtract { sum.sub_assign(&term) } else { sum.add_assign(&term) };
        if !ok { return None; }
        subtract = !subtract;
        denominator += 2;
        power = match power.mul_shift_192(&square) {
            Some(product) => product,
            None => shift(power.mul(&square)?),
        };
    }
}

fn series(tangent: &BigInt, working_precision: u32) -> BigInt {
    // Every power has the tangent's sign. Work on its magnitude so shifting
    // by the fixed-point precision is exactly division by 2^precision with
    // truncation toward zero, including for negative reduced arguments.
    let magnitude = tangent.magnitude();
    let square = (magnitude * magnitude) >> working_precision;
    let mut power = magnitude.clone();
    let mut sum = magnitude.clone();
    let mut subtract = true;
    let mut denominator = 3u32;
    loop {
        power = (power * &square) >> working_precision;
        let term = &power / denominator;
        if term.is_zero() { return BigInt::from_biguint(tangent.sign(), sum); }
        if subtract { sum -= term } else { sum += term }
        subtract = !subtract;
        denominator += 2;
    }
}

/// The same three constants as `CONSTANTS`, in the stack representation. The
/// values are below `2^192`, and each is used only when the working precision is
/// `PRECISION` (so no extra shift applies). `unit`, `half` and `quarter` are the
/// fixed-point powers `contour_angle_step` works in (`2^PRECISION`, and its
/// half and quarter): they are exactly determined by the precision, so they are
/// built once here instead of rebuilt on every ordinary direction.
struct FastConstants {
    pi: FixedUint,
    half_pi: FixedUint,
    half_angle: FixedUint,
    unit: FixedUint,
    half: FixedUint,
    quarter: FixedUint,
}

static FAST_CONSTANTS: LazyLock<FastConstants> = LazyLock::new(|| {
    let convert = |value: &BigInt| FixedUint::from_slice(&value.magnitude().to_u64_digits()).expect("constant fits");
    let c = &*CONSTANTS;
    let unit = FixedUint::from_u64(1).shl(PRECISION).expect("unit fits");
    FastConstants {
        pi: convert(&c.pi),
        half_pi: convert(&c.half_pi),
        half_angle: convert(&c.half_angle),
        half: unit.shr(1),
        quarter: unit.shr(2),
        unit,
    }
});

/// `contour_angle_step` with every value held in `FixedUint`, for the common
/// case: no extra precision, so the working precision is `PRECISION` and the
/// ratio, the reduced tangent and the angle all stay under `2^192`.
///
/// The operations are the reference's, in its order, with its truncation
/// points: `shift` places the numerator's significand at the working
/// precision, the division by the denominator's significand truncates, the
/// quarter-turn branch reduces the tangent the same way (and its numerator can
/// be negative, so the sign is carried beside the magnitude), and the sum is
/// signed against `half_pi` and `pi` at the end. Returns `None` if any value
/// would not fit, and the caller runs the arbitrary-precision path instead.
fn angle_step_fixed(numerator: (u64, i32), denominator: (u64, i32), transpose: bool, negative_x: bool, negative_y: bool,
    previous_angle: f64) -> Option<AngleStep> {
    let fast = &*FAST_CONSTANTS;
    let shift = PRECISION as i64 + (numerator.1 - denominator.1) as i64;
    let significand = FixedUint::from_u64(numerator.0);
    let shifted = if shift >= 0 { significand.shl(shift as u32)? } else { significand.shr((-shift) as u32) };
    let ratio = shifted.div_u64(denominator.0)?;
    let mut angle = if ratio > fast.quarter {
        let (numerator, negative) = if ratio > fast.half { (ratio.sub(&fast.half)?, false) } else { (fast.half.sub(&ratio)?, true) };
        // `(ratio - unit/2) * unit / (unit + ratio/2)`: the numerator is negative below a
        // half turn, and the reference's division truncates toward zero, so the sign is
        // carried beside the magnitude and the quotient's magnitude is the same either way.
        let (reduced, _) = numerator.shl(PRECISION)?.div_rem(&fast.unit.add(&ratio.shr(1))?)?;
        let series = SignedFixed::new(series_fixed_uint(&reduced)?, negative);
        SignedFixed::from_uint(fast.half_angle).add(&series)?
    } else {
        SignedFixed::from_uint(series_fixed_uint(&ratio)?)
    };
    if transpose { angle = SignedFixed::from_uint(fast.half_pi).sub(&angle)?; }
    if negative_x { angle = SignedFixed::from_uint(fast.pi).sub(&angle)?; }
    if negative_y { angle = angle.neg(); }
    let register = angle.round_significand(64)?;
    // The one input that can leave the fixed domain: a previous angle whose fixed-point
    // value needs more than the type holds. The caller then runs the BigInt path.
    let previous = FixedUint::from_f64_fixed(previous_angle, PRECISION)?;
    let previous = SignedFixed::new(previous, previous_angle < 0.0);
    let difference = register.sub(&previous)?.round_significand(53)?;
    Some(AngleStep { angle: register.to_f64(-(PRECISION as i64))?, difference: difference.to_f64(-(PRECISION as i64))? })
}

/// The axis directions' following double subtraction, in the stack
/// representation: the quarter-turn register is the constant rounded the same
/// way as the reference (`round_significand` on its magnitude), the previous
/// angle is its exact fixed-point value, and the difference is rounded to 53
/// bits before the conversion, which is the order the original instructions
/// use.
///
/// `None` when the previous angle's fixed-point value needs more limbs than
/// the type holds, exactly as the ordinary path declines; the caller then runs
/// the arbitrary-precision arm.
fn axis_difference_fixed(quarter: &FixedUint, negative: bool, previous_angle: f64) -> Option<f64> {
    let register = SignedFixed::new(quarter.round_significand(64)?, negative);
    let previous = SignedFixed::new(FixedUint::from_f64_fixed(previous_angle, PRECISION)?, previous_angle < 0.0);
    register.sub(&previous)?.round_significand(53)?.to_f64(-(PRECISION as i64))
}
struct Constants { pi: BigInt, half_pi: BigInt, half_angle: BigInt }

static CONSTANTS: LazyLock<Constants> = LazyLock::new(|| {
    let unit = unit();
    let pi = series(&(&unit / 5), PRECISION) * 16 - series(&(&unit / 239), PRECISION) * 4;
    let half_pi = &pi / 2;
    let half_angle = series(&(&unit / 2), PRECISION);
    Constants { pi, half_pi, half_angle }
});

/// Angle and the following double subtraction from the previous angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AngleStep { pub angle: f64, pub difference: f64 }

/// Original extended angle register and following double subtraction.
pub fn contour_angle_step(y: f64, x: f64, previous_angle: f64) -> AngleStep {
    if x == 0.0 || y == 0.0 {
        // Axis directions: atan2 is exact (0, +-pi/2, +-pi) on every libm.
        let angle = y.atan2(x);
        if angle == 0.0 { return AngleStep { angle, difference: angle - previous_angle }; }
        let quarter_turn = angle == std::f64::consts::PI || angle == -std::f64::consts::PI;
        let fast = &*FAST_CONSTANTS;
        let quarter = if quarter_turn { &fast.pi } else { &fast.half_pi };
        if let Some(difference) = axis_difference_fixed(quarter, angle < 0.0, previous_angle) {
            return AngleStep { angle, difference };
        }
        let c = &*CONSTANTS;
        let quarter = if quarter_turn { &c.pi } else { &c.half_pi };
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
    if extra == 0 {
        if let Some(step) = angle_step_fixed(numerator, denominator, transpose, x < 0.0, y < 0.0, previous_angle) {
            return step;
        }
    }
    angle_step_reference(numerator, denominator, transpose, x < 0.0, y < 0.0, previous_angle, extra)
}

/// The original evaluation, kept whole as the oracle the fast path is checked
/// against: arbitrary-precision `BigInt` throughout, with the extra-precision
/// shift that subnormal directions need.
fn angle_step_reference(numerator: (u64, i32), denominator: (u64, i32), transpose: bool, negative_x: bool, negative_y: bool,
    previous_angle: f64, extra: u32) -> AngleStep {
    let c = &*CONSTANTS;
    let working_precision = PRECISION + extra;
    let working_unit = unit() << extra;
    let shift = working_precision as i64 + (numerator.1 - denominator.1) as i64;
    let significand = BigInt::from(numerator.0);
    let shifted = if shift >= 0 { significand << shift as u64 } else { significand >> (-shift) as u64 };
    let ratio = shifted / BigInt::from(denominator.0);
    let mut angle = if ratio > &working_unit / 4 {
        let reduced = (&ratio - &working_unit / 2) * &working_unit / (&working_unit + &ratio / 2);
        (&c.half_angle << extra) + series(&reduced, working_precision)
    } else { series(&ratio, working_precision) };
    if transpose { angle = (&c.half_pi << extra) - angle; }
    if negative_x { angle = (&c.pi << extra) - angle; }
    let register = round_significand(&if negative_y { -angle } else { angle }, 64);
    let difference = round_significand(&(&register - fixed_double_at(previous_angle, working_precision)), 53);
    AngleStep {
        angle: binary_double(&register, -(working_precision as i64)),
        difference: binary_double(&difference, -(working_precision as i64)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::One;

    // Retain the original signed, division-based evaluator as an exact oracle.
    fn signed_division_series(tangent: &BigInt, precision: u32) -> BigInt {
        let unit = BigInt::one() << precision;
        let square = tangent * tangent / &unit;
        let mut power = tangent.clone();
        let mut sum = tangent.clone();
        let mut subtract = true;
        let mut denominator = BigInt::from(3);
        loop {
            power = &power * &square / &unit;
            let term = &power / &denominator;
            if term.is_zero() { return sum; }
            if subtract { sum -= term } else { sum += term }
            subtract = !subtract;
            denominator += 2;
        }
    }

    fn assert_series(tangent: BigInt, precision: u32) {
        let expected = signed_division_series(&tangent, precision);
        if precision == PRECISION {
            let magnitude = FixedUint::from_slice(&tangent.magnitude().to_u64_digits()).unwrap();
            let sum = series_fixed_uint(&magnitude).unwrap();
            assert_eq!(num_bigint::BigUint::from_slice(&sum.to_u32_digits()), *expected.magnitude());
        }
        assert_eq!(series(&tangent, precision), expected,
            "tangent={tangent}, precision={precision}");
    }

    #[test]
    fn magnitude_series_preserves_signed_truncation() {
        // Exhaust the small fixed-point grids: negative right shifts would
        // round down instead of toward zero, so these catch that substitution.
        for precision in [4, 8, 12] {
            let half = 1i64 << (precision - 1);
            for tangent in -half..=half { assert_series(BigInt::from(tangent), precision); }
        }
    }

    #[test]
    fn magnitude_series_matches_boundaries_and_extra_precision() {
        // 2045 is the largest binary_parts exponent gap between finite f64
        // directions, including the smallest subnormal and largest normal.
        for precision in [PRECISION, PRECISION + 1, PRECISION + 64, PRECISION + 1074, PRECISION + 2045] {
            let unit = BigInt::one() << precision;
            let anchors = [BigInt::zero(), BigInt::one(), BigInt::one() << (precision / 2),
                &unit / 239, &unit / 5, &unit / 4, &unit / 3, &unit / 2];
            for anchor in anchors {
                for offset in [-1, 0, 1] {
                    let tangent: BigInt = &anchor + offset;
                    assert_series(tangent.clone(), precision);
                    assert_series(-tangent, precision);
                }
            }
        }
    }

    #[test]
    fn magnitude_series_matches_varied_direction_ratios() {
        let mut seed = 0x517cc1b727220a95u64;
        for precision in [PRECISION, PRECISION + 64, PRECISION + 1074, PRECISION + 2045] {
            let unit = BigInt::one() << precision;
            for _ in 0..128 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let ratio: BigInt = (BigInt::from(seed >> 11) * &unit) >> 53;
                // Exercise the same ratio reduction as contour_angle_step,
                // including its negative arguments when 1/4 < ratio < 1/2.
                let tangent = if ratio > &unit / 4 {
                    (&ratio - &unit / 2) * &unit / (&unit + &ratio / 2)
                } else { ratio };
                assert_series(tangent.clone(), precision);
                assert_series(-tangent, precision);
            }
        }
    }
}

#[cfg(test)]
mod angle_equivalence {
    //! The fast path against the retained arbitrary-precision reference, over
    //! directions of every kind the engine meets: the axes, each quadrant,
    //! transposed and not, the ratio boundaries the branch chooses on, ratios
    //! near zero and near one, subnormals and extreme exponents, and a spread
    //! of previous angles (the register subtraction is where a signed rounding
    //! difference would show).
    use super::*;

    fn check(y: f64, x: f64, previous: f64) {
        let expected = contour_angle_step_reference(y, x, previous);
        let actual = contour_angle_step(y, x, previous);
        assert_eq!(actual.angle.to_bits(), expected.angle.to_bits(), "angle y={y:e} x={x:e} prev={previous:e}");
        assert_eq!(actual.difference.to_bits(), expected.difference.to_bits(), "difference y={y:e} x={x:e} prev={previous:e}");
    }

    /// The original axis branch, in arbitrary precision, as the oracle for the
    /// stack version `contour_angle_step` now takes there.
    fn axis_step_reference(y: f64, x: f64, previous_angle: f64) -> AngleStep {
        let c = &*CONSTANTS;
        let angle = y.atan2(x);
        if angle == 0.0 { return AngleStep { angle, difference: angle - previous_angle }; }
        let quarter = if angle == std::f64::consts::PI || angle == -std::f64::consts::PI { &c.pi } else { &c.half_pi };
        let register = round_significand(&if angle < 0.0 { -quarter.clone() } else { quarter.clone() }, 64);
        let difference = binary_double(&round_significand(&(register - fixed_double_at(previous_angle, PRECISION)), 53), -(PRECISION as i64));
        AngleStep { angle, difference }
    }

    /// `contour_angle_step`'s reference arm, reachable from tests.
    fn contour_angle_step_reference(y: f64, x: f64, previous_angle: f64) -> AngleStep {
        if x == 0.0 || y == 0.0 {
            return axis_step_reference(y, x, previous_angle);
        }
        let (absolute_x, absolute_y) = (x.abs(), y.abs());
        let transpose = absolute_y > absolute_x;
        let (smaller, larger) = if transpose { (absolute_x, absolute_y) } else { (absolute_y, absolute_x) };
        let approximate_ratio = smaller / larger;
        let numerator = binary_parts(smaller);
        let denominator = binary_parts(larger);
        let extra: u32 = if approximate_ratio < 2f64.powi(-64) && !transpose && x > 0.0 {
            (denominator.1 - numerator.1).max(0) as u32
        } else { 0 };
        angle_step_reference(numerator, denominator, transpose, x < 0.0, y < 0.0, previous_angle, extra)
    }

    fn previous_angles() -> Vec<f64> {
        let mut values = vec![0.0, -0.0, 1.0, -1.0, std::f64::consts::PI, -std::f64::consts::PI,
            std::f64::consts::PI / 2.0, -std::f64::consts::PI / 2.0, 1e-300, -1e-300, 1e300, -1e300,
            f64::MIN_POSITIVE, -f64::MIN_POSITIVE, 5e-324, -5e-324];
        let mut seed = 0x243f6a8885a308d3u64;
        for _ in 0..16 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let angle = ((seed >> 11) as f64 / (1u64 << 53) as f64) * 8.0 - 4.0;
            values.push(angle);
        }
        values
    }

    #[test]
    fn the_fast_arm_is_taken_for_ordinary_directions() {
        // The equivalence test above would pass trivially if the fast path always declined,
        // so pin that it accepts the directions the engine actually meets.
        let directions: [(f64, f64); 12] = [(1.0, 1.0), (1.0, 0.3), (0.3, 1.0), (-1.0, 0.7), (0.7, -1.0),
            (0.9999, -0.0001), (1e-8, 1.0), (1.0, 1e-8), (-0.5, -0.5), (0.25, 1.0), (0.5, 1.0), (0.75, 1.0)];
        for &(y, x) in &directions {
            let numerator = binary_parts(y.abs().min(x.abs()));
            let denominator = binary_parts(y.abs().max(x.abs()));
            let transpose = y.abs() > x.abs();
            assert!(angle_step_fixed(numerator, denominator, transpose, x < 0.0, y < 0.0, 0.0).is_some(),
                "fast arm declined y={y} x={x}");
        }
    }

    #[test]
    fn fast_angle_step_matches_the_reference_on_many_directions() {
        let previous = previous_angles();
        let mut checked = 0usize;
        // The axes and each quadrant's exact unit directions.
        for &y in &[0.0, -0.0, 1.0, -1.0, 0.5, -0.5] {
            for &x in &[0.0, -0.0, 1.0, -1.0, 0.5, -0.5] {
                for &p in &previous { check(y, x, p); checked += 1; }
            }
        }
        // Fixed direction families: the branch boundaries, ratio extremes, and a grid of
        // exponents including subnormals and the widest finite range.
        let mut directions: Vec<(f64, f64)> = Vec::new();
        for &ratio in &[0.0, 5e-324, 1e-320, 2f64.powi(-64), 2f64.powi(-64) * 1.0000000000000002, 0.25 - 1e-16,
            0.25, 0.25 + 1e-16, 0.5 - 1e-16, 0.5, 0.5 + 1e-16, 1.0 - 1e-16, 1.0] {
            directions.push((ratio, 1.0));
            directions.push((1.0, ratio));
            directions.push((-ratio, 1.0));
            directions.push((ratio, -1.0));
        }
        for exponent in [-1074i32, -1073, -1022, -1000, -512, -64, -1, 0, 1, 64, 512, 1023] {
            let scale = 2f64.powi(exponent);
            if !scale.is_finite() || scale == 0.0 { continue; }
            for &(y, x) in &[(1.0, 1.0), (1.0, 0.3), (0.3, 1.0), (-1.0, 0.7), (0.7, -1.0), (0.9999, -0.0001)] {
                directions.push((y * scale, x * scale));
            }
        }
        for (y, x) in directions {
            for &p in &previous { check(y, x, p); checked += 1; }
        }
        // A deterministic pseudo-random sweep over the whole quarter-plane and its signs.
        let mut seed = 0x13198a2e03707344u64;
        for _ in 0..2048 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let y = ((seed >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0;
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let x = ((seed >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0;
            if x == 0.0 || y == 0.0 { continue; }
            for &p in &previous { check(y, x, p); checked += 1; }
        }
        assert!(checked >= 8192, "checked only {checked} directions");
    }

    #[test]
    fn cached_powers_match_the_per_call_construction() {
        // The scale operand used to be rebuilt on every ordinary direction. It is
        // exactly determined by `PRECISION`, so the cached copy must be the same
        // fixed-point value word for word, including its used length and ordering.
        let unit = FixedUint::from_u64(1).shl(PRECISION).expect("unit fits");
        let fast = &*FAST_CONSTANTS;
        for (cached, expected) in [(&fast.unit, &unit), (&fast.half, &unit.shr(1)), (&fast.quarter, &unit.shr(2))] {
            assert_eq!(cached.limbs(), expected.limbs());
            assert_eq!(cached.limbs().len(), expected.limbs().len());
            assert_eq!(cached.cmp(expected), std::cmp::Ordering::Equal);
            assert_eq!(cached, expected);
        }
        // A power of two at the precision is four limbs, top limb one; its half
        // and quarter shift that bit down one and two places, which drops the
        // now-empty top limb (`shr` normalizes).
        assert_eq!(fast.unit.limbs(), &[0, 0, 0, 1]);
        assert_eq!(fast.half.limbs(), &[0, 0, 1 << 63]);
        assert_eq!(fast.quarter.limbs(), &[0, 0, 1 << 62]);
        assert!(fast.quarter < fast.half && fast.half < fast.unit);
    }

    #[test]
    fn quarter_branch_boundary_uses_the_cached_threshold() {
        // `ratio > quarter` chooses the reduction; both sides of that compare must
        // produce the reference's bytes, so a stale or misplaced threshold shows.
        let previous = previous_angles();
        let fast = &*FAST_CONSTANTS;
        for &ratio in &[0.25f64, 0.25 + 1e-16, 0.5 - 1e-16, 0.5, 0.75] {
            let numerator = binary_parts(ratio);
            let denominator = binary_parts(1.0);
            let step = angle_step_fixed(numerator, denominator, false, false, false, 0.0)
                .expect("the quarter branch stays in the fixed domain");
            assert!(step.angle.is_finite());
            for &p in &previous { check(ratio, 1.0, p); }
        }
        // The compare accepts exactly the same set as the value it replaced.
        let unit = FixedUint::from_u64(1).shl(PRECISION).expect("unit fits");
        let quarter = unit.shr(2);
        for &ratio in &[0.0f64, 0.24999999999999997, 0.25, 0.25000000000000006, 0.5, 1.0] {
            let value = FixedUint::from_f64_fixed(ratio, PRECISION).expect("ratio fits");
            assert_eq!(value > fast.quarter, value > quarter, "ratio={ratio}");
        }
    }

    #[test]
    fn the_axis_fast_arm_is_taken_and_exact_for_extremes() {
        // Every axis direction takes the stack arm, and the arm's conversion is
        // exact against the arbitrary-precision one, including subnormal and
        // huge previous angles (where `from_f64_fixed` can decline).
        let mut accepted = 0usize;
        for &y in &[0.0f64, -0.0, 1.0, -1.0, 1e-300, -1e-300] {
            for &x in &[0.0f64, -0.0, 1.0, -1.0, 1e300, -1e300] {
                if x != 0.0 && y != 0.0 { continue; }
                for &p in &previous_angles() {
                    // `atan2(0, 0)` is zero and returns before the conversion.
                    if y.atan2(x) == 0.0 { check(y, x, p); continue; }
                    let fast = axis_difference_fixed(
                        if y.atan2(x).abs() == std::f64::consts::PI { &FAST_CONSTANTS.pi } else { &FAST_CONSTANTS.half_pi },
                        y.atan2(x) < 0.0, p);
                    let reference = axis_step_reference(y, x, p);
                    match fast {
                        Some(difference) => {
                            assert_eq!(difference.to_bits(), reference.difference.to_bits(), "axis y={y:e} x={x:e} prev={p:e}");
                            accepted += 1;
                        }
                        None => assert_eq!(
                            fixed_double_at(p, PRECISION).magnitude().bits() > 8 * 64, true,
                            "axis declined though the value fits: prev={p:e}"),
                    }
                    check(y, x, p);
                }
            }
        }
        assert!(accepted > 100, "axis fast arm declined too often: {accepted}");
    }
}
