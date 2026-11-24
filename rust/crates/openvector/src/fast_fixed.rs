//! Eight inline 64-bit limbs hold the fixed-point magnitudes for contour angles.
//! Arithmetic that exceeds the available precision returns `None` so the
//! caller can recompute with arbitrary precision. Signs are stored separately
//! to preserve truncation toward zero in the arctangent series.

use crate::extended_arithmetic::{binary_parts, pow2};

const LIMBS: usize = 8;

/// `(high << 64 | low) / divisor` and its remainder, for `high < divisor`.
///
/// The dividend is a 128-bit value whose high word is below the divisor, so
/// the quotient fits one word and the hardware's 128-by-64 divide computes it
/// in one instruction. Written as inline assembly because the compiler cannot
/// see the `high < divisor` invariant through the `u128` division and emits a
/// call to `__udivti3` instead; the invariant is the caller's, and every call
/// site here establishes it by construction (the remainder of a previous
/// division by the same divisor).
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn div_wide(high: u64, low: u64, divisor: u64) -> (u64, u64) {
    debug_assert!(high < divisor);
    let quotient: u64;
    let remainder: u64;
    // SAFETY: `high < divisor` keeps the quotient below `2^64`, which is the
    // only precondition of `div`; the divisor is nonzero at every call site.
    // `div` leaves the flags undefined, so they are not claimed preserved.
    unsafe {
        core::arch::asm!(
            "div {divisor}",
            divisor = in(reg) divisor,
            inout("rax") low => quotient,
            inout("rdx") high => remainder,
            options(nomem, nostack),
        );
    }
    (quotient, remainder)
}

/// The same division on targets without the 128-by-64 instruction: the
/// compiler's own `u128` division, which is exact for the same invariant.
#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
fn div_wide(high: u64, low: u64, divisor: u64) -> (u64, u64) {
    debug_assert!(high < divisor);
    let dividend = ((high as u128) << 64) | (low as u128);
    let quotient = dividend / (divisor as u128);
    (quotient as u64, (dividend - quotient * (divisor as u128)) as u64)
}

/// A sign and a magnitude, the shape the reference's signed values take in the
/// angle step: the series works on magnitudes, the sign is applied at the end,
/// and the register subtraction is the one place a negative value meets a
/// positive one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SignedFixed {
    magnitude: FixedUint,
    negative: bool,
}

/// Unsigned big integer with an inline, fixed-capacity magnitude.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FixedUint {
    limbs: [u64; LIMBS],
    len: u8,
}

impl std::fmt::Debug for FixedUint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FixedUint(")?;
        for i in (0..self.len as usize).rev() {
            write!(f, "{:016x}", self.limbs[i])?;
        }
        write!(f, ")")
    }
}

impl FixedUint {
    pub const ZERO: FixedUint = FixedUint { limbs: [0; LIMBS], len: 0 };

    pub fn is_zero(&self) -> bool {
        self.len == 0
    }

    /// Number of limbs in use; zero has none.
    pub fn limbs(&self) -> &[u64] {
        &self.limbs[..self.len as usize]
    }

    pub fn bit_len(&self) -> u32 {
        if self.len == 0 {
            return 0;
        }
        (self.len as u32 - 1) * 64 + (64 - self.limbs[self.len as usize - 1].leading_zeros())
    }

    /// The value as one limb when it fits, `Some(0)` for zero. The series' tail
    /// reads this once per term to pick the single-divide arm.
    pub(crate) fn single_limb(&self) -> Option<u64> {
        if self.len <= 1 { Some(self.limbs[0]) } else { None }
    }

    /// The value's low three limbs, zero-filled above its length. The series'
    /// tail multiplies a one-limb power by the square through these words.
    pub(crate) fn low_three_limbs(&self) -> [u64; 3] {
        [self.limbs[0], self.limbs[1], self.limbs[2]]
    }

    fn normalize(&mut self) {
        while self.len > 0 && self.limbs[self.len as usize - 1] == 0 {
            self.len -= 1;
        }
    }

    fn from_parts(limbs: [u64; LIMBS], len: u8) -> Self {
        let mut value = FixedUint { limbs, len };
        value.normalize();
        value
    }

    /// Normalize the used prefix of a result written directly into its limbs.
    fn finish(mut value: FixedUint, len: usize) -> Self {
        value.len = len as u8;
        value.normalize();
        value
    }

    /// `None` when the value needs more limbs than the type holds.
    pub fn from_slice(limbs: &[u64]) -> Option<Self> {
        if limbs.len() > LIMBS {
            return None;
        }
        let mut array = [0u64; LIMBS];
        array[..limbs.len()].copy_from_slice(limbs);
        Some(FixedUint::from_parts(array, limbs.len() as u8))
    }

    pub fn from_u64(value: u64) -> Self {
        let mut array = [0u64; LIMBS];
        array[0] = value;
        FixedUint::from_parts(array, 1)
    }

    /// Little-endian 32-bit digits, `BigUint`'s own layout.
    #[cfg(test)]
    pub(crate) fn to_u32_digits(&self) -> Vec<u32> {
        let mut digits = Vec::with_capacity(self.limbs().len() * 2);
        for limb in self.limbs() {
            digits.push(*limb as u32);
            digits.push((*limb >> 32) as u32);
        }
        while digits.last() == Some(&0) {
            digits.pop();
        }
        digits
    }

    /// `self + other`, or `None` if the sum needs more than `LIMBS` limbs.
    pub fn add(&self, other: &Self) -> Option<Self> {
        let mut value = FixedUint::ZERO;
        let mut carry = 0u64;
        let len = self.len.max(other.len) as usize;
        for i in 0..len {
            let a = self.limbs[i];
            let b = other.limbs.get(i).copied().unwrap_or(0);
            let (s, c1) = a.overflowing_add(b);
            let (s, c2) = s.overflowing_add(carry);
            value.limbs[i] = s;
            carry = (c1 as u64) + (c2 as u64);
        }
        if carry != 0 {
            if len == LIMBS {
                return None;
            }
            value.limbs[len] = carry;
            return Some(FixedUint::finish(value, len + 1));
        }
        Some(FixedUint::finish(value, len))
    }

    /// `self += other`, in place, or `false` if the sum needs more than `LIMBS`
    /// limbs; `self` then holds the wrapped low limbs, so a caller that refuses
    /// must not read it.
    ///
    /// The same carry pass `add` runs, writing into the accumulator the caller
    /// already owns instead of a fresh value returned through an `Option`: the
    /// series' sum changes on every iteration, and `FixedUint` is 72 bytes, so a
    /// returned value costs a copy out of the callee's frame and another into
    /// the caller's slot. Every value's limbs above `len` are zero (the
    /// constructors and `finish` all build on a zeroed array), so the wider
    /// operand's tail needs no test here either.
    pub(crate) fn add_assign(&mut self, other: &Self) -> bool {
        let len = self.len.max(other.len) as usize;
        let mut carry = 0u64;
        for i in 0..len {
            let a = self.limbs[i];
            let b = other.limbs.get(i).copied().unwrap_or(0);
            let (s, c1) = a.overflowing_add(b);
            let (s, c2) = s.overflowing_add(carry);
            self.limbs[i] = s;
            carry = (c1 as u64) + (c2 as u64);
        }
        if carry != 0 {
            if len == LIMBS {
                return false;
            }
            self.limbs[len] = carry;
            self.len = (len + 1) as u8;
            return true;
        }
        self.len = len as u8;
        self.normalize();
        true
    }

    /// `self -= other`, in place, or `false` when `self < other`.
    ///
    /// `sub`'s borrow pass with the result landing in `self`; `self`'s own
    /// length bounds the pass exactly as it does there.
    pub(crate) fn sub_assign(&mut self, other: &Self) -> bool {
        if *self < *other {
            return false;
        }
        // The series' sum minus a term: `other` is the shorter operand almost
        // every time, so the low limbs are one slice pass with no per-limb
        // `get`/`unwrap_or`, and the borrow then runs on through the rest of
        // `self`'s own limbs. The values are the same as the general pass's.
        let shared = other.len as usize;
        let mut borrow = 0u64;
        for i in 0..shared {
            let (d, b1) = self.limbs[i].overflowing_sub(other.limbs[i]);
            let (d, b2) = d.overflowing_sub(borrow);
            self.limbs[i] = d;
            borrow = (b1 as u64) + (b2 as u64);
        }
        if borrow != 0 {
            for i in shared..self.len as usize {
                let (d, b) = self.limbs[i].overflowing_sub(borrow);
                self.limbs[i] = d;
                borrow = b as u64;
                if borrow == 0 { break; }
            }
        }
        self.normalize();
        true
    }

    /// `self += small` for a one-limb addend, or `false` if the sum needs more
    /// than `LIMBS` limbs.
    ///
    /// The series' tail adds a term that is one limb, so the general pass's
    /// per-limb `get`/`unwrap_or` and its `max` of two lengths are both dead
    /// work there. The values are the same as `add_assign`'s.
    pub(crate) fn add_u64_assign(&mut self, small: u64) -> bool {
        let mut carry = small;
        for i in 0..self.len as usize {
            if carry == 0 {
                return true;
            }
            let (sum, overflow) = self.limbs[i].overflowing_add(carry);
            self.limbs[i] = sum;
            carry = overflow as u64;
        }
        if carry != 0 {
            if self.len as usize == LIMBS {
                return false;
            }
            self.limbs[self.len as usize] = carry;
            self.len += 1;
        }
        true
    }

    /// `self -= small` for a one-limb subtrahend, or `false` when
    /// `self < small`; `self` then holds the wrapped low limbs, as
    /// `sub_assign` leaves it.
    pub(crate) fn sub_u64_assign(&mut self, small: u64) -> bool {
        if self.len == 0 {
            return small == 0;
        }
        let (difference, mut borrow) = self.limbs[0].overflowing_sub(small);
        self.limbs[0] = difference;
        if borrow {
            for i in 1..self.len as usize {
                let (difference, b) = self.limbs[i].overflowing_sub(1);
                self.limbs[i] = difference;
                borrow = b;
                if !borrow {
                    break;
                }
            }
            if borrow {
                return false;
            }
        }
        self.normalize();
        true
    }

    /// `self - other`, or `None` when `self < other`.
    pub fn sub(&self, other: &Self) -> Option<Self> {
        if *self < *other {
            return None;
        }
        let mut value = FixedUint::ZERO;
        let mut borrow = 0u64;
        for i in 0..self.len as usize {
            let a = self.limbs[i];
            let b = other.limbs.get(i).copied().unwrap_or(0);
            let (d, b1) = a.overflowing_sub(b);
            let (d, b2) = d.overflowing_sub(borrow);
            value.limbs[i] = d;
            borrow = (b1 as u64) + (b2 as u64);
        }
        Some(FixedUint::finish(value, self.len as usize))
    }

    /// `self << shift`, or `None` if any set bit would be shifted out.
    pub fn shl(&self, shift: u32) -> Option<Self> {
        if self.len == 0 || shift == 0 {
            return Some(*self);
        }
        if shift > (LIMBS as u32) * 64 - self.bit_len() {
            return None;
        }
        let whole = (shift / 64) as usize;
        let bits = shift % 64;
        let mut value = FixedUint::ZERO;
        for i in (0..self.len as usize).rev() {
            let shifted = (self.limbs[i] as u128) << bits;
            value.limbs[i + whole] |= shifted as u64;
            if bits != 0 && i + whole + 1 < LIMBS {
                value.limbs[i + whole + 1] |= (shifted >> 64) as u64;
            }
        }
        Some(FixedUint::finish(value, (self.len as usize + whole + 1).min(LIMBS)))
    }

    /// `self >> shift`: a floor shift, as a non-negative value's shift is.
    pub fn shr(&self, shift: u32) -> Self {
        if self.len == 0 || shift == 0 {
            return *self;
        }
        let whole = (shift / 64) as usize;
        if whole >= self.len as usize {
            return FixedUint::ZERO;
        }
        let bits = shift % 64;
        let mut value = FixedUint::ZERO;
        let used = self.len as usize - whole;
        for i in 0..used {
            let low = self.limbs[i + whole];
            let high = if i + whole + 1 < self.len as usize { self.limbs[i + whole + 1] } else { 0 };
            value.limbs[i] = if bits == 0 { low } else { (low >> bits) | (high << (64 - bits)) };
        }
        FixedUint::finish(value, used)
    }

    /// `self >> (64 * limbs)`: the same floor shift as `shr` for a shift that
    /// is a whole number of limbs, with no per-limb bit branch.
    ///
    /// The series shifts by `PRECISION`, which is a whole number of limbs, so
    /// this is the shape it needs; `shr` stays the general form.
    pub fn shr_limbs(&self, limbs: u32) -> Self {
        let whole = limbs as usize;
        if whole >= self.len as usize {
            return FixedUint::ZERO;
        }
        let mut value = FixedUint::ZERO;
        let used = self.len as usize - whole;
        value.limbs[..used].copy_from_slice(&self.limbs[whole..self.len as usize]);
        FixedUint::finish(value, used)
    }

    /// `self * other`, or `None` if the product needs more than `LIMBS` limbs.
    pub fn mul(&self, other: &Self) -> Option<Self> {
        if self.len == 0 || other.len == 0 {
            return Some(FixedUint::ZERO);
        }
        // The product needs at least the sum of the bit lengths minus one.
        if self.bit_len() + other.bit_len() > (LIMBS as u32) * 64 + 1 {
            return None;
        }
        let mut value = FixedUint::ZERO;
        for i in 0..self.len as usize {
            let mut carry = 0u64;
            for j in 0..other.len as usize {
                let slot = i + j;
                let product = (self.limbs[i] as u128) * (other.limbs[j] as u128)
                    + (value.limbs[slot] as u128) + (carry as u128);
                value.limbs[slot] = product as u64;
                carry = (product >> 64) as u64;
            }
            // The carry lands on a limb no earlier row has written: row `i`'s
            // inner loop stops at `i + other.len - 1`, and row `i - 1`'s carry
            // went to `i + other.len - 1`. So the target is zero and the store
            // is exact. At capacity, a nonzero final carry signals overflow.
            let slot = i + other.len as usize;
            if slot < LIMBS {
                value.limbs[slot] = carry;
            } else if carry != 0 {
                return None;
            }
        }
        Some(FixedUint::finish(value, (self.len as usize + other.len as usize).min(LIMBS)))
    }

    /// `(self * other) >> 192` for the arctangent series' power update, where
    /// both operands are at most three limbs (the tangent's magnitude is below
    /// `2^190` and the square below `2^384`, so the product is below `2^576`
    /// and the shift leaves at most three limbs).
    ///
    /// The general `mul` walks `self.len * other.len` limb pairs with a
    /// bounds-checked `value.limbs[slot]` store and a `bit_len` guard, then
    /// `shr_limbs` copies the result down. Here the three-by-three product is
    /// formed in six named words with no indexing, and the shift is a read of
    /// the words above the third. `None` when either operand is wider than
    /// three limbs or the product's top word would carry out of the six, which
    /// is exactly the domain the caller's `mul`/`shr_limbs` pair refuses.
    pub fn mul_shift_192(&self, other: &Self) -> Option<Self> {
        // The series' tail: once the power has shrunk below one limb, the
        // product is a single 64x192 multiply and the shift is a read of the
        // words above the third. This is the common case — the power falls by
        // a factor of sixteen a term, so most of the series' terms are here.
        if self.len <= 1 {
            if other.len > 3 {
                return None;
            }
            let a = self.limbs[0] as u128;
            let mut product = [0u64; 4];
            let mut carry = 0u128;
            for j in 0..3 {
                let sum = a * (other.limbs[j] as u128) + (product[j] as u128) + carry;
                product[j] = sum as u64;
                carry = sum >> 64;
            }
            product[3] = carry as u64;
            return Some(FixedUint::finish(FixedUint { limbs: [product[3], 0, 0, 0, 0, 0, 0, 0], len: 0 }, 1));
        }
        if self.len > 3 || other.len > 3 {
            return None;
        }
        let a = [self.limbs[0], self.limbs[1], self.limbs[2]];
        let b = [other.limbs[0], other.limbs[1], other.limbs[2]];
        let mut product = [0u64; 6];
        let mut carry = 0u128;
        for i in 0..3 {
            let mut row = 0u128;
            for j in 0..3 {
                let slot = i + j;
                let sum = (a[i] as u128) * (b[j] as u128) + (product[slot] as u128) + row;
                product[slot] = sum as u64;
                row = sum >> 64;
            }
            let slot = i + 3;
            let sum = (product[slot] as u128) + row + carry;
            product[slot] = sum as u64;
            carry = sum >> 64;
        }
        if carry != 0 {
            return None;
        }
        Some(FixedUint::finish(FixedUint { limbs: [product[3], product[4], product[5], 0, 0, 0, 0, 0], len: 0 }, 3))
    }

    /// Truncating division by a small denominator, as the reference's
    /// `&power / denominator` truncates a non-negative value.
    pub fn div_u64(&self, small: u64) -> Option<Self> {
        if small == 0 {
            return None;
        }
        let mut value = FixedUint::ZERO;
        let mut remainder = 0u64;
        for i in (0..self.len as usize).rev() {
            let (quotient, next) = div_wide(remainder, self.limbs[i], small);
            value.limbs[i] = quotient;
            remainder = next;
        }
        Some(FixedUint::finish(value, self.len as usize))
    }

    /// `self / divisor` and `self % divisor`, truncating; `None` on a zero
    /// divisor.
    ///
    /// Knuth's algorithm D over 64-bit limbs: both operands are normalised so
    /// the divisor's top limb has its high bit set, each quotient digit is
    /// estimated from the top two limbs of the partial remainder, then
    /// corrected by one multiply-subtract, with the add-back branch for the
    /// estimate that came out one too high.
    pub fn div_rem(&self, divisor: &Self) -> Option<(Self, Self)> {
        if divisor.is_zero() {
            return None;
        }
        if *self < *divisor {
            return Some((FixedUint::ZERO, *self));
        }
        let divisor_len = divisor.len as usize;
        if divisor_len == 1 {
            let d = divisor.limbs[0];
            let mut value = FixedUint::ZERO;
            let mut remainder = 0u64;
            for i in (0..self.len as usize).rev() {
                let (quotient, next) = div_wide(remainder, self.limbs[i], d);
                value.limbs[i] = quotient;
                remainder = next;
            }
            return Some((FixedUint::finish(value, self.len as usize), FixedUint::from_u64(remainder)));
        }
        let shift = divisor.limbs[divisor_len - 1].leading_zeros();
        let numerator = self.shl(shift)?;
        let denominator = divisor.shl(shift)?;
        let n = denominator.len as usize;
        let m = numerator.len as usize - n;
        let mut u = [0u64; LIMBS + 1];
        u[..numerator.len as usize].copy_from_slice(numerator.limbs());
        let mut quotient = [0u64; LIMBS];
        let v_top = denominator.limbs[n - 1] as u128;
        let v_next = denominator.limbs[n - 2] as u128;
        for j in (0..=m).rev() {
            let high = (u[j + n] as u128) << 64 | (u[j + n - 1] as u128);
            let mut qhat = high / v_top;
            let mut rhat = high % v_top;
            if qhat > u64::MAX as u128 {
                qhat = u64::MAX as u128;
                rhat = high - qhat * v_top;
            }
            while rhat <= u64::MAX as u128 && qhat * v_next > ((rhat << 64) | (u[j + n - 2] as u128)) {
                qhat -= 1;
                rhat += v_top;
            }
            let mut borrow = 0i128;
            let mut carry = 0u128;
            for i in 0..n {
                let product = qhat * (denominator.limbs[i] as u128) + carry;
                carry = product >> 64;
                let difference = (u[j + i] as i128) - (product as u64 as i128) - borrow;
                u[j + i] = difference as u64;
                borrow = if difference < 0 { 1 } else { 0 };
            }
            let difference = (u[j + n] as i128) - (carry as i128) - borrow;
            u[j + n] = difference as u64;
            if difference < 0 {
                qhat -= 1;
                let mut carry = 0u64;
                for i in 0..n {
                    let (sum, c1) = u[j + i].overflowing_add(denominator.limbs[i]);
                    let (sum, c2) = sum.overflowing_add(carry);
                    u[j + i] = sum;
                    carry = (c1 as u64) + (c2 as u64);
                }
                u[j + n] = u[j + n].wrapping_add(carry);
            }
            quotient[j] = qhat as u64;
        }
        let remainder = FixedUint::from_slice(&u[..n])?.shr(shift);
        let quotient = FixedUint::from_slice(&quotient[..=m])?;
        Some((quotient, remainder))
    }

    /// Round to `digits` significant bits, ties to even, as
    /// `extended_arithmetic::round_significand` rounds a magnitude.
    ///
    /// The reference's `significand << shift` is `self` with its low `shift`
    /// bits cleared, and its `remainder`/`half` comparison is a read of those
    /// cleared bits: above the half is "round bit set and something below it
    /// set", exactly a half is "round bit set and nothing below it", and a tie
    /// rounds up only when the significand is odd. Deciding from the bits in
    /// place removes the `sub` and both `shl` calls — including the
    /// `1 << (shift - 1)` half word — so the value is masked, maybe incremented,
    /// and returned without a scratch array.
    pub fn round_significand(&self, digits: u32) -> Option<Self> {
        let length = self.bit_len();
        if length <= digits {
            return Some(*self);
        }
        let shift = length - digits;
        let whole = (shift / 64) as usize;
        let bits = shift % 64;
        // `whole` reaches `self.len` only for `digits == 0`, where the surviving
        // significand is empty; nothing below may index that word then.
        let within = whole < self.len as usize;
        let mut value = *self;
        for word in value.limbs[..whole].iter_mut() {
            *word = 0;
        }
        if bits != 0 && within {
            value.limbs[whole] &= !((1u64 << bits) - 1);
        }
        if self.rounds_up(shift) {
            let mut carry = 1u64 << bits;
            for word in &mut value.limbs[whole..] {
                let (sum, overflow) = word.overflowing_add(carry);
                *word = sum;
                carry = overflow as u64;
                if carry == 0 { break; }
            }
            if carry != 0 { return None; }
        }
        Some(FixedUint::finish(value, (self.len as usize + 1).min(LIMBS)))
    }

    /// Whether the reference's shift-out/compare/shift-back rounds up: bit
    /// `shift - 1` is set and either something below it is set or the surviving
    /// value is odd. `shift` must be at least 1.
    ///
    /// This is the whole of the reference's `remainder > half || (remainder ==
    /// half && odd)` test, read from the input's own bits, so the callers that
    /// only need the decision (the subnormal arm of `to_f64_scaled`) do not
    /// build the `sub`/`shl`/half-word temporaries to ask it.
    #[inline]
    fn rounds_up(&self, shift: u32) -> bool {
        let round_word = ((shift - 1) / 64) as usize;
        // A round bit above the value is clear: nothing to round.
        if round_word >= self.len as usize {
            return false;
        }
        let round_shift = (shift - 1) % 64;
        if (self.limbs[round_word] >> round_shift) & 1 == 0 {
            return false;
        }
        let below = if round_shift == 0 { 0 } else { (1u64 << round_shift) - 1 };
        if self.limbs[..round_word].iter().any(|&word| word != 0) || self.limbs[round_word] & below != 0 {
            return true;
        }
        let whole = (shift / 64) as usize;
        whole < self.len as usize && (self.limbs[whole] >> (shift % 64)) & 1 == 1
    }

    /// `self * 2^exponent` as the nearest double, ties to even, including
    /// subnormals, as `extended_arithmetic::binary_double` converts a magnitude.
    ///
    /// The reference rounds the magnitude to 53 bits, then shifts the rounded
    /// value down by whatever the rounding added. Both steps are reads of this
    /// value's own bits — the rounded significand is the 53-bit window at
    /// `length - 53`, and a round up that carries out of it is the reference's
    /// `2^length`, whose significand is `2^52` one bit higher — so the
    /// conversion builds no `FixedUint` at all: no `round_significand` result,
    /// no `shr` of it, no `Option` around either.
    pub fn to_f64_scaled(&self, exponent: i64) -> Option<f64> {
        if self.is_zero() {
            return Some(0.0);
        }
        let length = self.bit_len();
        let highest = length as i64 - 1 + exponent;
        if highest < -1022 {
            let shift = -1074 - exponent;
            let units = if shift > 0 {
                let distance = shift as u32;
                // `units = self >> distance` is at most 52 bits here, so it is
                // a read of the two words the shift can touch.
                let mut units = self.window(distance);
                if self.rounds_up(distance) {
                    units += 1;
                }
                units
            } else {
                self.shl((-shift) as u32)?.limbs().first().copied().unwrap_or(0)
            };
            // The reference scales the unit count by `f64::from_bits(1)`, the
            // smallest subnormal, which is `2^-1074`.
            return Some(units as f64 * f64::from_bits(1));
        }
        let mut shift = length.saturating_sub(53);
        let mut significand = self.window(shift);
        if shift > 0 && self.rounds_up(shift) {
            significand += 1;
            if significand == 1u64 << 53 {
                // The reference's rounded value is `2^length`, which needs one
                // more bit: the type refuses exactly at full capacity, and the
                // caller falls back to the arbitrary-precision arm.
                if length >= (LIMBS as u32) * 64 {
                    return None;
                }
                significand >>= 1;
                shift += 1;
            }
        }
        Some(significand as f64 * pow2(exponent + shift as i64))
    }

    /// The 64 bits starting at bit `shift`, as `self >> shift` truncated to one
    /// word. Callers only use it where the shifted value is known to fit.
    #[inline]
    fn window(&self, shift: u32) -> u64 {
        let whole = (shift / 64) as usize;
        let bits = shift % 64;
        let low = self.limbs.get(whole).copied().unwrap_or(0);
        if bits == 0 {
            return low;
        }
        let high = self.limbs.get(whole + 1).copied().unwrap_or(0);
        (low >> bits) | (high << (64 - bits))
    }
}

impl Ord for FixedUint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.len != other.len {
            return self.len.cmp(&other.len);
        }
        for i in (0..self.len as usize).rev() {
            if self.limbs[i] != other.limbs[i] {
                return self.limbs[i].cmp(&other.limbs[i]);
            }
        }
        std::cmp::Ordering::Equal
    }
}

impl PartialOrd for FixedUint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl SignedFixed {

    pub fn new(magnitude: FixedUint, negative: bool) -> Self {
        SignedFixed { magnitude, negative: negative && !magnitude.is_zero() }
    }

    pub fn from_uint(magnitude: FixedUint) -> Self {
        SignedFixed::new(magnitude, false)
    }

    pub fn neg(&self) -> Self {
        SignedFixed::new(self.magnitude, !self.negative)
    }

    /// `self + other`, exactly, or `None` if the magnitude would not fit.
    pub fn add(&self, other: &Self) -> Option<Self> {
        if self.negative == other.negative {
            return Some(SignedFixed::new(self.magnitude.add(&other.magnitude)?, self.negative));
        }
        if self.magnitude >= other.magnitude {
            Some(SignedFixed::new(self.magnitude.sub(&other.magnitude)?, self.negative))
        } else {
            Some(SignedFixed::new(other.magnitude.sub(&self.magnitude)?, other.negative))
        }
    }

    /// `self - other`, exactly.
    pub fn sub(&self, other: &Self) -> Option<Self> {
        self.add(&other.neg())
    }

    /// Round to `digits` significant bits, ties to even, keeping the sign.
    pub fn round_significand(&self, digits: u32) -> Option<Self> {
        Some(SignedFixed::new(self.magnitude.round_significand(digits)?, self.negative))
    }

    /// The value as a double, as `extended_arithmetic::binary_double` converts a
    /// signed fixed-point value.
    pub fn to_f64(&self, exponent: i64) -> Option<f64> {
        let value = self.magnitude.to_f64_scaled(exponent)?;
        Some(if self.negative { -value } else { value })
    }
}

impl FixedUint {
    /// The magnitude of a double's fixed-point value at `precision`, as
    /// `extended_arithmetic::fixed_double_at` builds it, without a `BigInt`.
    /// The sign stays with the caller, as it does everywhere else here.
    ///
    /// `None` when the value needs more limbs than the type holds (a large
    /// magnitude at a high precision), which tells the caller to fall back.
    pub fn from_f64_fixed(value: f64, precision: u32) -> Option<Self> {
        if value == 0.0 {
            return Some(FixedUint::ZERO);
        }
        let (significand, exponent) = binary_parts(value.abs());
        let shift = precision as i64 + exponent as i64;
        // The significand is one double's 53 bits, so the placement is a write
        // of at most two words: build the value in place instead of going
        // through `from_u64` (a whole array) and then a general `shl`/`shr`.
        let bit_len = 64 - significand.leading_zeros();
        let mut limbs = [0u64; LIMBS];
        if shift >= 0 {
            if bit_len as i64 + shift > (LIMBS as i64) * 64 {
                return None;
            }
            let whole = (shift / 64) as usize;
            let bits = (shift % 64) as u32;
            limbs[whole] = significand << bits;
            // The guard above keeps every set bit inside the type, so a high
            // word at the last index would be zero: never index past it.
            let used = if bits == 0 {
                whole + 1
            } else if whole + 1 < LIMBS {
                limbs[whole + 1] = significand >> (64 - bits);
                whole + 2
            } else {
                whole + 1
            };
            Some(FixedUint::finish(FixedUint { limbs, len: 0 }, used.min(LIMBS)))
        } else {
            let down = (-shift) as u32;
            if down < 64 {
                limbs[0] = significand >> down;
            }
            Some(FixedUint::finish(FixedUint { limbs, len: 0 }, 1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extended_arithmetic::PRECISION;
    use num_bigint::BigUint;
    use num_traits::Zero;

    fn big(value: &FixedUint) -> BigUint {
        BigUint::from_slice(&value.to_u32_digits())
    }

    fn fixed(value: &BigUint) -> FixedUint {
        FixedUint::from_slice(&value.to_u64_digits()).expect("value fits")
    }

    /// A deterministic stream of values, including the widths where a limb
    /// boundary, a carry or a borrow is most likely to be wrong.
    struct Stream(u64);
    impl Stream {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 ^ (self.0 >> 29)
        }
        fn value(&mut self, limbs: usize) -> BigUint {
            let mut digits = Vec::with_capacity(limbs * 2);
            for _ in 0..limbs * 2 {
                let word = self.next();
                digits.push(word as u32);
                digits.push((word >> 32) as u32);
            }
            while digits.last() == Some(&0) { digits.pop(); }
            BigUint::from_slice(&digits)
        }
    }

    #[test]
    fn add_sub_mul_match_biguint() {
        let mut stream = Stream(0x9e3779b97f4a7c15);
        for limbs in 1..=2 {
            for _ in 0..64 {
                let (al, bl) = (limbs, limbs);
                let a = stream.value(al);
                let b = stream.value(bl);
                let (fa, fb) = (fixed(&a), fixed(&b));
                assert_eq!(big(&fa.add(&fb).unwrap()), &a + &b, "add {a} {b}");
                if a >= b {
                    assert_eq!(big(&fa.sub(&fb).unwrap()), &a - &b, "sub {a} {b}");
                }
                assert_eq!(big(&fa.mul(&fb).unwrap()), &a * &b, "mul {a} {b}");
                let small = stream.next();
                if small != 0 {
                    assert_eq!(big(&fa.div_u64(small).unwrap()), &a / BigUint::from(small), "div_u64 {a} {small}");
                }
            }
        }
    }

    #[test]
    fn limb_shift_matches_the_general_shift() {
        // `shr_limbs(n)` must equal `shr(64 * n)` for every value and shift the
        // series can hit, including shifts past the top limb.
        let mut stream = Stream(0x51ab7c0de9f31027);
        for limbs in 1..=3 {
            for _ in 0..64 {
                let a = stream.value(limbs);
                let fa = fixed(&a);
                for whole in [0u32, 1, 2, 3, 4] {
                    assert_eq!(fa.shr_limbs(whole), fa.shr(whole * 64), "shr_limbs {a} >> {whole} limbs");
                }
            }
        }
        assert_eq!(FixedUint::from_u64(7).shr_limbs(8), FixedUint::ZERO, "past the top limb");
    }

    #[test]
    fn in_place_sum_matches_the_returning_forms() {
        // The series' in-place accumulators must agree with `add`/`sub` bit for
        // bit, including the refusals and the zero results.
        let mut stream = Stream(0x2f6a1c93b7d40e51);
        for _ in 0..512 {
            let a_limbs = 1 + (stream.next() as usize % 3);
            let b_limbs = 1 + (stream.next() as usize % 3);
            let fa = fixed(&stream.value(a_limbs));
            let fb = fixed(&stream.value(b_limbs));

            let mut sum = fa;
            let ok = sum.add_assign(&fb);
            assert_eq!(ok, fa.add(&fb).is_some(), "add_assign refusal");
            if let Some(expected) = fa.add(&fb) {
                assert_eq!(sum, expected, "add_assign");
            }

            let mut difference = fa;
            let ok = difference.sub_assign(&fb);
            assert_eq!(ok, fa.sub(&fb).is_some(), "sub_assign refusal");
            if let Some(expected) = fa.sub(&fb) {
                assert_eq!(difference, expected, "sub_assign");
            }
        }
        // Equal operands subtract to zero, which must leave a canonical zero.
        let value = fixed(&BigUint::from(0x1234_5678_9abc_def0u64));
        let mut zero = value;
        assert!(zero.sub_assign(&value));
        assert!(zero.is_zero(), "x - x is the canonical zero");
        assert_eq!(zero.limbs().len(), 0);
        // A sum that would need a limb the type does not hold is refused, and
        // the accumulator then holds the wrapped low limbs (the contract the
        // series depends on: it returns `None` and never reads `sum` again).
        let full = FixedUint::from_slice(&[u64::MAX; LIMBS]).unwrap();
        let mut carry = full;
        assert!(!carry.add_assign(&FixedUint::from_u64(1)), "sum past the top limb");
        assert_eq!(carry.limbs(), &[0u64; LIMBS][..], "the wrapped low limbs");
    }

    #[test]
    fn one_limb_accumulators_match_the_general_forms() {
        // The series' tail adds and subtracts a one-limb term, and divides a
        // one-limb power. Each must agree with the general form bit for bit,
        // including the refusals and the canonical zero.
        let mut stream = Stream(0x7f4a7c159e3779b9);
        for _ in 0..512 {
            let limbs = 1 + (stream.next() as usize % 3);
            let fa = fixed(&stream.value(limbs));
            let small = stream.next();

            let mut sum = fa;
            let ok = sum.add_u64_assign(small);
            let expected = fa.add(&FixedUint::from_u64(small));
            assert_eq!(ok, expected.is_some(), "add_u64_assign refusal");
            if let Some(expected) = expected {
                assert_eq!(sum, expected, "add_u64_assign {fa:?} + {small}");
            }

            let mut difference = fa;
            let ok = difference.sub_u64_assign(small);
            let expected = fa.sub(&FixedUint::from_u64(small));
            assert_eq!(ok, expected.is_some(), "sub_u64_assign refusal");
            if let Some(expected) = expected {
                assert_eq!(difference, expected, "sub_u64_assign {fa:?} - {small}");
            }

            if let Some(limb) = fa.single_limb() {
                assert_eq!(limb, big(&fa).to_u64_digits().first().copied().unwrap_or(0), "single_limb");
                if small != 0 {
                    assert_eq!(limb / small, (big(&fa) / BigUint::from(small)).to_u64_digits().first().copied().unwrap_or(0),
                        "the one-limb quotient {fa:?} / {small}");
                }
            }
        }
        // A one-limb addend that carries out of the top limb is refused, and a
        // subtrahend above the value is refused.
        let full = FixedUint::from_slice(&[u64::MAX; LIMBS]).unwrap();
        let mut carry = full;
        assert!(!carry.add_u64_assign(1), "one-limb sum past the top limb");
        let mut under = FixedUint::from_u64(3);
        assert!(!under.sub_u64_assign(4), "one-limb difference below zero");
        let mut zero = FixedUint::from_u64(9);
        assert!(zero.sub_u64_assign(9));
        assert!(zero.is_zero(), "x - x is the canonical zero");
        assert_eq!(zero.limbs().len(), 0);
        assert_eq!(FixedUint::ZERO.single_limb(), Some(0), "zero is one limb");
    }

    #[test]
    fn growth_past_capacity_is_refused_not_wrapped() {
        // Every operation that can grow a value says so instead of wrapping.
        let full = FixedUint::from_slice(&[u64::MAX; LIMBS]).unwrap();
        assert!(full.add(&FixedUint::from_u64(1)).is_none(), "add past the top limb");
        assert!(full.mul(&FixedUint::from_u64(2)).is_none(), "mul past the top limb");
        assert!(full.mul(&full).is_none(), "square past the top limb");
        let one = FixedUint::from_u64(1);
        assert_eq!(full.mul(&one), Some(full));
        let high = one.shl(256).unwrap();
        assert_eq!(high.mul(&one.shl(192).unwrap()), one.shl(448));
        assert!(one.shl(u32::MAX).is_none());
        assert!(full.shl(1).is_none(), "shift past the top limb");
        assert!(FixedUint::from_slice(&[1u64; LIMBS + 1]).is_none(), "input too wide");
        assert!(FixedUint::from_u64(1).sub(&FixedUint::from_u64(2)).is_none(), "underflow");
        assert!(FixedUint::from_u64(1).div_u64(0).is_none(), "zero divisor");
        assert!(FixedUint::from_u64(1).div_rem(&FixedUint::ZERO).is_none(), "zero divisor");
    }

    #[test]
    fn shifts_match_biguint() {
        let mut stream = Stream(0x2545f4914f6cdd1d);
        for limbs in 1..=2 {
            for _ in 0..32 {
                let a = stream.value(limbs);
                let fa = fixed(&a);
                for shift in [0u32, 1, 7, 63, 64, 65, 127, 128, 191, 192, 200] {
                    assert_eq!(fa.shr(shift), FixedUint::from_slice(&(&a >> shift).to_u64_digits()).unwrap(), "shr {a} {shift}");
                    if fa.bit_len() + shift <= (LIMBS as u32) * 64 {
                        assert_eq!(big(&fa.shl(shift).unwrap()), &a << shift, "shl {a} {shift}");
                    } else {
                        assert!(fa.shl(shift).is_none(), "shl {a} {shift} should not fit");
                    }
                }
            }
        }
    }

    #[test]
    fn mul_shift_192_matches_the_general_product_and_shift() {
        // The series' power update, against `mul` + `shr_limbs` on the same
        // operands. The caller falls back to that pair whenever this returns
        // `None`, so a `None` is never wrong; what must hold is that every
        // `Some` is the general pair's value, and that the arm the series
        // spends most of its terms in — a power below one limb — is served.
        let mut stream = Stream(0x9e3779b97f4a7c15);
        let mut served = 0;
        for right in 1..=3 {
            for _ in 0..32 {
                let power = fixed(&stream.value(1));
                let square = fixed(&stream.value(right));
                let expected = power.mul(&square).map(|p| p.shr_limbs(PRECISION / 64));
                if let Some(actual) = power.mul_shift_192(&square) {
                    assert_eq!(Some(actual), expected, "single-limb arm");
                    served += 1;
                }
            }
        }
        assert!(served > 0, "the single-limb arm serves the series' common case");
        // Wider than three limbs is outside the specialised domain, and the
        // caller's general pair takes it.
        let four = FixedUint::from_slice(&[1, 0, 0, 1]).unwrap();
        assert!(four.mul_shift_192(&FixedUint::from_u64(1)).is_none(), "operand too wide");
    }

    #[test]
    fn division_matches_biguint_including_knuth_corrections() {
        let mut stream = Stream(0x517cc1b727220a95);
        for _ in 0..256 {
            let (al, bl) = (1 + (stream.next() % 2) as usize, 1 + (stream.next() % 2) as usize);
            let a = stream.value(al);
            let b = stream.value(bl);
            if b.is_zero() { continue; }
            let (quotient, remainder) = fixed(&a).div_rem(&fixed(&b)).unwrap();
            assert_eq!(big(&quotient), &a / &b, "quotient {a} / {b}");
            assert_eq!(big(&remainder), &a % &b, "remainder {a} % {b}");
        }
        // The top two limbs suggest quotient 2, but the dividend is 2*d - 1.
        let d = FixedUint::from_slice(&[1, 0, 1 << 63]).unwrap();
        let n = FixedUint::from_slice(&[1, 0, 0, 1]).unwrap();
        let one = FixedUint::from_u64(1);
        assert_eq!(n.div_rem(&d), Some((one, d.sub(&one).unwrap())));
        let cases: [(u64, u64); 7] = [
            (u64::MAX, 1),
            (u64::MAX, u64::MAX),
            (0x8000_0000_0000_0000, 0x8000_0000_0000_0001),
            (0xffff_ffff_ffff_fffe, 0x8000_0000_0000_0001),
            (0x0000_0000_ffff_ffff, 0x0000_0000_0000_0001),
            (0x7fff_ffff_ffff_ffff, 0x8000_0000_0000_0000),
            (0x8000_0000_0000_0001, 0x8000_0000_0000_0000),
        ];
        for (a, b) in cases {
            let (quotient, remainder) = FixedUint::from_u64(a).div_rem(&FixedUint::from_u64(b)).unwrap();
            assert_eq!(big(&quotient), BigUint::from(a) / BigUint::from(b), "quotient {a} / {b}");
            assert_eq!(big(&remainder), BigUint::from(a) % BigUint::from(b), "remainder {a} % {b}");
        }
    }

    #[test]
    fn rounding_matches_round_significand() {
        use crate::extended_arithmetic::round_significand;
        let mut stream = Stream(0xdeadbeefcafebabe);
        for _ in 0..256 {
            let al = 1 + (stream.next() % 2) as usize;
            let a = stream.value(al);
            let fa = fixed(&a);
            for digits in [1u32, 2, 53, 63, 64, 65, 128] {
                let expected = round_significand(&num_bigint::BigInt::from(a.clone()), digits as u64);
                assert_eq!(big(&fa.round_significand(digits).unwrap()), expected.magnitude().clone(), "round {a} {digits}");
            }
        }
    }

    /// The boundaries the fused rounding has to get exactly right: an exact half
    /// that ties up and one that ties down, a value one bit above a half, the
    /// largest magnitude that fits (where a round up is refused, as the
    /// reference's `shl` refuses it), the same just below the top, and every cut
    /// position across the limb boundaries, all against the reference.
    #[test]
    fn rounding_hits_the_carry_and_tie_boundaries() {
        use crate::extended_arithmetic::round_significand as reference;
        let check = |value: &BigUint, digits: u32| {
            let fixed_value = fixed(value);
            let expected = reference(&num_bigint::BigInt::from(value.clone()), digits as u64);
            let fits = expected.magnitude().bits() <= (LIMBS as u64) * 64;
            match fixed_value.round_significand(digits) {
                Some(actual) => {
                    assert!(fits, "round {value} {digits} returned a value the reference drops");
                    assert_eq!(big(&actual), expected.magnitude().clone(), "round {value} {digits}");
                }
                None => assert!(!fits, "round {value} {digits} refused a {}-bit value", expected.magnitude().bits()),
            }
        };
        let power = |n: u32| BigUint::from(1u64) << n;
        // Exact power, then a tie at the top of the first limb and across the
        // 64-bit boundary, then one bit above each tie.
        check(&power(63), 1);
        check(&(power(63) + power(61)), 2);
        check(&(power(64) + power(63)), 1);
        check(&(power(64) + (BigUint::from(3u64) << 63)), 1);
        check(&(power(65) + power(64) + power(62)), 2);
        // Every value all ones: rounding to 53 bits needs 513 bits, which the
        // reference produces and the type cannot hold, so it must be refused.
        let full = FixedUint::from_slice(&[u64::MAX; LIMBS]).unwrap();
        assert!(full.round_significand(53).is_none(), "all ones rounded to 53 bits needs 513 bits");
        let full_big = power((LIMBS as u32) * 64) - BigUint::from(1u64);
        check(&(power(511) + power(458) + BigUint::from(1u64)), 53);
        check(&full_big, 512);
        check(&full_big, 511);
        check(&full_big, 1);
        check(&full_big, 0);
        check(&(full_big.clone() - power((LIMBS as u32) * 64 - 1)), 53);
        // Every cut position across the first few limb boundaries.
        let mut stream = Stream(0x243f6a8885a308d3);
        for _ in 0..16 {
            let a = stream.value(3);
            for digits in 1..=192 {
                check(&a, digits);
            }
        }
    }

    #[test]
    fn f64_conversion_matches_fixed_double_at() {
        use crate::extended_arithmetic::fixed_double_at;
        let values: [f64; 20] = [0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 3.0, -3.5, 1e-300, -1e-300, 1e300,
            f64::MIN_POSITIVE, -f64::MIN_POSITIVE, 5e-324, -5e-324, std::f64::consts::PI,
            -std::f64::consts::PI, 2.0, 1.0 / 3.0, -1.0 / 3.0];
        let mut stream = Stream(0xfeedfacadebeef01);
        let mut values = values.to_vec();
        for _ in 0..64 {
            values.push(((stream.next() >> 11) as f64 / (1u64 << 53) as f64) * 1e3 - 5e2);
        }
        for value in values {
            let expected = fixed_double_at(value, 192);
            let actual = FixedUint::from_f64_fixed(value, 192);
            match actual {
                Some(actual) => assert_eq!(big(&actual), expected.magnitude().clone(), "value {value:e}"),
                // A large magnitude at high precision legitimately needs more limbs; the
                // caller falls back, and the value must really be too wide.
                None => assert!(expected.magnitude().bits() > (LIMBS as u64) * 64, "value {value:e} refused though it fits"),
            }
        }
    }

    /// `to_f64_scaled` against the reference `binary_double`, over the whole
    /// exponent range the conversion can see: normal values, the subnormal
    /// boundary and below it, exact ties at the round bit, values one bit either
    /// side of a tie, and the carry out of the 53-bit window.
    #[test]
    fn f64_scaled_matches_binary_double_at_every_boundary() {
        use crate::extended_arithmetic::binary_double;
        let check = |value: &BigUint, exponent: i64| {
            assert!(value.bits() <= 512, "test value too wide: {} bits at 2^{}", value.bits(), exponent);
            let fixed_value = fixed(value);
            let expected = binary_double(&num_bigint::BigInt::from(value.clone()), exponent);
            match fixed_value.to_f64_scaled(exponent) {
                Some(actual) => assert_eq!(actual.to_bits(), expected.to_bits(), "to_f64 {value} * 2^{exponent}"),
                // The only refusal is a round up out of the full 512 bits, which
                // the reference turns into exactly `2^length`; the caller then
                // runs the arbitrary-precision arm.
                None => {
                    let length = fixed_value.bit_len() as i64;
                    assert_eq!(expected.to_bits(), pow2(exponent + length).to_bits(),
                        "to_f64 {value} * 2^{exponent} refused a value the reference keeps");
                }
            }
        };
        let power = |n: u32| BigUint::from(1u64) << n;
        // Exact powers across the normal/subnormal boundary and the smallest
        // subnormal, at the exponents that put them there.
        for bits in [1u32, 2, 52, 53, 54, 63, 64, 65, 100, 191, 192, 193, 511, 512] {
            for exponent in [-1074i64, -1073, -1072, -1023, -1022, -1021, -192, -1, 0, 1, 100] {
                check(&power(bits - 1), exponent);
                check(&(power(bits) - BigUint::from(1u64)), exponent);
            }
        }
        // Ties and near-ties at the 53-bit window, in the normal range and in
        // the subnormal arm, including the carry that makes `2^length`.
        for length in [54u32, 55, 64, 65, 128, 192, 193, 512] {
            let top = power(length - 1);
            let half = power(length - 54);
            for delta in [0u32, 1] {
                let tie = &top + &half + BigUint::from(delta as u64);
                for exponent in [-1074i64, -1023, -1022, -1000, -192, 0] {
                    check(&tie, exponent);
                }
            }
            // All ones: the round up carries out of the window.
            check(&(power(length) - BigUint::from(1u64)), -192);
            check(&(power(length) - BigUint::from(1u64)), -1022);
        }
        // Random magnitudes at every width, at exponents that land in both arms.
        let mut stream = Stream(0x5deece66d8c7f3a1);
        // `Stream::value(n)` returns `n * 128` bits, so four fills the type.
        for limbs in 1..=LIMBS / 2 {
            for _ in 0..24 {
                let value = stream.value(limbs);
                for exponent in [-1100i64, -1074, -1050, -1023, -1022, -900, -192, -64, 0, 64] {
                    check(&value, exponent);
                }
            }
        }
        // Every cut position of a full-width value, so each limb boundary and
        // each round-bit position is compared, not just the sampled ones.
        for _ in 0..4 {
            let value = stream.value(LIMBS / 2);
            for exponent in [-1074i64, -1022, -192, 0] {
                check(&value, exponent);
            }
        }
        // The subnormal arm's own exponent sweep: every exponent from -720 down
        // to -1100, where the shift crosses the 64-bit word boundaries.
        let wide = stream.value(LIMBS / 2);
        for exponent in -1100i64..=-720 {
            check(&wide, exponent);
        }
    }

    #[test]
    fn signed_add_sub_keep_the_reference_sign() {
        let mut stream = Stream(0x0123456789abcdef);
        for _ in 0..128 {
            let (al, bl) = (1 + (stream.next() % 3) as usize, 1 + (stream.next() % 3) as usize);
            let a = stream.value(al);
            let b = stream.value(bl);
            let (sa, sb) = (stream.next() & 1 == 1, stream.next() & 1 == 1);
            let fa = SignedFixed::new(fixed(&a), sa);
            let fb = SignedFixed::new(fixed(&b), sb);
            let expected = |value: BigUint, negative: bool| {
                if negative { -(num_bigint::BigInt::from(value)) } else { num_bigint::BigInt::from(value) }
            };
            let check = |value: &SignedFixed| {
                if value.negative { -(num_bigint::BigInt::from(big(&value.magnitude))) } else { num_bigint::BigInt::from(big(&value.magnitude)) }
            };
            assert_eq!(check(&fa.add(&fb).unwrap()), expected(a.clone(), sa) + expected(b.clone(), sb), "add {a} {sa} {b} {sb}");
            assert_eq!(check(&fa.sub(&fb).unwrap()), expected(a, sa) - expected(b, sb), "sub");
        }
    }
}
