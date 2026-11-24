//! Port of `src/blend-solver.mjs`: per-pixel color blend fitting over the
//! native small-matrix routines (column-major Cholesky, reverse-order forward
//! substitution and an active-set nonnegativity loop).
#[cfg(test)]
fn collect_blend_indices_reference(indices: &[i32], width: usize, height: usize, pixel: usize, directions: &[(i64, i64)],
    source: i32, target: i32) -> Vec<i32> {
    let remap = |r: i32| if r == source { target } else { r };
    let mut selected = vec![remap(indices[pixel])];
    let (x, y) = ((pixel % width) as i64, (pixel / width) as i64);
    for (dx, dy) in directions {
        let (column, row) = (x + dx, y + dy);
        if column < 0 || column >= width as i64 || row < 0 || row >= height as i64 { continue; }
        let color = remap(indices[(row * width as i64 + column) as usize]);
        if !selected.contains(&color) {
            selected.push(color);
            if selected.len() == 9 { break; }
        }
    }
    selected
}

const BYTE_SCALE: f32 = 1.0 / 255.0;

pub const EIGHT_DIRECTIONS: [(i64, i64); 8] = [(1, 0), (0, 1), (-1, 0), (0, -1), (-1, 1), (1, -1), (-1, -1), (1, 1)];
pub const TWENTY_FOUR_DIRECTIONS: [(i64, i64); 24] = [(1, 0), (0, 1), (-1, 0), (0, -1), (-1, 1), (1, -1), (-1, -1), (1, 1),
    (-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2), (2, -2), (2, -1), (2, 0), (2, 1), (2, 2),
    (-1, -2), (0, -2), (1, -2), (-1, 2), (0, 2), (1, 2)];

/// A sample whose first two channels stay double and last two are float32,
/// matching the native register/storage mix. Values are held as f64.
pub fn write_blend_sample(bgra: &[u8], offset: usize, sample: &mut [f64; 4]) {
    for channel in 0..4 {
        let value = bgra[offset + channel] as f64 * BYTE_SCALE as f64;
        sample[channel] = if channel < 2 { value } else { value as f32 as f64 };
    }
}

pub fn collect_blend_indices(indices: &[i32], width: usize, height: usize, pixel: usize, directions: &[(i64, i64)],
    source: i32, target: i32) -> Vec<i32> {
    let mut selected = [0; MAX_FIT_COLORS];
    let len = collect_blend_indices_into(indices, width, height, pixel, directions, source, target, &mut selected);
    selected[..len].to_vec()
}

/// The selected colors, written into the caller's nine-entry buffer: the
/// center first, then each direction's remapped color in order, skipping
/// repeats and stopping at nine. Returns the used length. The buffer is the
/// caller's, so a per-pixel call allocates nothing.
pub(crate) fn collect_blend_indices_into(indices: &[i32], width: usize, height: usize, pixel: usize, directions: &[(i64, i64)],
    source: i32, target: i32, selected: &mut [i32; MAX_FIT_COLORS]) -> usize {
    let remap = |r: i32| if r == source { target } else { r };
    selected[0] = remap(indices[pixel]);
    let mut len = 1;
    let (x, y) = ((pixel % width) as i64, (pixel / width) as i64);
    for (dx, dy) in directions {
        let (column, row) = (x + dx, y + dy);
        if column < 0 || column >= width as i64 || row < 0 || row >= height as i64 { continue; }
        let color = remap(indices[(row * width as i64 + column) as usize]);
        if !selected[..len].contains(&color) {
            selected[len] = color;
            len += 1;
            if len == selected.len() { break; }
        }
    }
    len
}

pub fn dot_float(first: &[f32], second: &[f32]) -> f32 {
    let mut value = first[0] * second[0];
    for channel in 1..4 { value += first[channel] * second[channel]; }
    value
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct BlendFit { pub color: Vec<f32>, pub weights: Vec<f64>, pub self_weight: f64 }

fn fit_two_colors_parts(pixel: &[f64; 4], colors: &[f32]) -> ([f32; 4], [f64; 2], f64) {
    // The two channel vectors are exactly four entries, so they are built as
    // arrays: the same four expressions in the same channel order, with no
    // heap block and no `Range`/`Map`/`SpecFromIterNested` machinery per fit.
    // `dot_float` takes a slice, so `&array` is the same four `f32` operands in
    // the same order and every accumulation is bit-identical.
    let difference: [f32; 4] = std::array::from_fn(|c| colors[c + 4] - colors[c]);
    let norm = dot_float(&difference, &difference);
    if norm <= 0.0 { return ([colors[0], colors[1], colors[2], colors[3]], [0.5, 0.5], 0.5); }
    let relative: [f32; 4] = std::array::from_fn(|c| (pixel[c] as f32) - colors[c]);
    let norm64 = norm as f64;
    let mut second = dot_float(&difference, &relative) as f64 / norm64;
    second = (1.0 - (second + second)) * (0.01 / (norm64 + 0.02)) + second;
    second = second.clamp(0.0, 1.0);
    let first = 1.0 - second;
    let rounded = second as f32;
    let color: [f32; 4] = std::array::from_fn(|c| colors[c] + rounded * difference[c]);
    (color, [first, second], first)
}

/// The owned form of the two-color fit, kept for the equivalence test that
/// pins `fit_two_colors_parts` against the retained `Vec` oracle.
#[cfg(test)]
fn fit_two_colors(pixel: &[f64; 4], colors: &[f32]) -> BlendFit {
    let (color, weights, self_weight) = fit_two_colors_parts(pixel, colors);
    BlendFit { color: color.to_vec(), weights: weights.to_vec(), self_weight }
}

#[cfg(test)]
mod two_color_equivalence {
    //! `fit_two_colors` against the retained `Vec` form, bit for bit. The
    //! production version builds its two channel vectors and its answer as
    //! four-entry arrays; the oracle below is the original `collect` form, kept
    //! verbatim, so a changed operand, order or rounding fails here.
    use super::*;

    fn rng(seed: u64) -> impl FnMut() -> u64 {
        let mut state = seed | 1;
        move || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state }
    }

    fn fit_two_colors_reference(pixel: &[f64; 4], colors: &[f32]) -> BlendFit {
        let difference: Vec<f32> = (0..4).map(|c| colors[c + 4] - colors[c]).collect();
        let norm = dot_float(&difference, &difference);
        if norm <= 0.0 { return BlendFit { color: colors[..4].to_vec(), weights: vec![0.5, 0.5], self_weight: 0.5 }; }
        let relative: Vec<f32> = (0..4).map(|c| (pixel[c] as f32) - colors[c]).collect();
        let norm64 = norm as f64;
        let mut second = dot_float(&difference, &relative) as f64 / norm64;
        second = (1.0 - (second + second)) * (0.01 / (norm64 + 0.02)) + second;
        second = second.clamp(0.0, 1.0);
        let first = 1.0 - second;
        let rounded = second as f32;
        let color = (0..4).map(|c| colors[c] + rounded * difference[c]).collect();
        BlendFit { color, weights: vec![first, second], self_weight: first }
    }

    fn same(got: &BlendFit, want: &BlendFit) -> bool {
        got.color.len() == want.color.len() && got.weights.len() == want.weights.len()
            && got.color.iter().zip(&want.color).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.weights.iter().zip(&want.weights).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.self_weight.to_bits() == want.self_weight.to_bits()
    }

    #[test]
    fn arrays_match_the_collect_form_bit_for_bit() {
        let mut next = rng(0x2c01_7e5a_0000_0001);
        let mut checked = 0usize;
        let mut degenerate = 0usize;
        for _ in 0..4000 {
            // Two colors, four channels each, from the byte grid the engine
            // actually feeds a fit, plus the odd non-byte value.
            let mut colors = [0.0f32; 8];
            for value in colors.iter_mut() {
                *value = match next() % 8 {
                    0 => (next() % 256) as f32 * f32::from_bits((1.0f32 / 255.0).to_bits()),
                    1 => 0.0,
                    2 => 1.0,
                    3 => -0.0,
                    4 => f32::MIN_POSITIVE,
                    5 => f32::MAX,
                    6 => f32::NAN,
                    _ => (next() % 1000) as f32 * 0.001,
                };
            }
            let mut pixel = [0.0f64; 4];
            for channel in 0..4 {
                pixel[channel] = match next() % 4 {
                    0 => (next() % 256) as f64 / 255.0,
                    1 => 0.0,
                    2 => 1.0,
                    _ => (next() % 1000) as f64 * 0.001,
                };
            }
            let got = fit_two_colors(&pixel, &colors);
            let want = fit_two_colors_reference(&pixel, &colors);
            assert!(same(&got, &want), "fit differs for colors={colors:?} pixel={pixel:?}");
            if got.self_weight.to_bits() == 0.5f64.to_bits() { degenerate += 1; }
            checked += 1;
        }
        assert_eq!(checked, 4000);
        assert!(degenerate > 0, "the degenerate arm must be exercised: {degenerate}");
    }
}

// Column-major storage and loop order follow the native small-matrix routines.
#[cfg(test)]
fn factor_positive_matrix(matrix: &[f64], size: usize) -> Result<Vec<f64>, &'static str> {
    let mut factor = matrix.to_vec();
    for row in 0..size {
        for column in 0..row {
            let offset = column * size + row;
            for previous in 0..column {
                factor[offset] -= factor[previous * size + row] * factor[previous * size + column];
            }
            factor[offset] /= factor[column * size + column];
        }
        let diagonal = row * size + row;
        for column in 0..row {
            let value = factor[column * size + row];
            factor[diagonal] -= value * value;
        }
        if factor[diagonal] <= 0.0 { return Err("Color-blend matrix is not positive definite."); }
        factor[diagonal] = factor[diagonal].sqrt();
    }
    Ok(factor)
}

/// The factorization with the system size known at compile time. The
/// operations, their order and every operand are the ones
/// `factor_positive_matrix_in_place` runs for the same `size`: only the row
/// stride and the loop bounds become constants, so the index arithmetic folds
/// and the bounds checks on `factor` are resolved against the caller's slice
/// once instead of per element.
fn factor_positive_const<const N: usize>(factor: &mut [f64]) -> Result<(), &'static str> {
    debug_assert!(factor.len() >= N * N, "the factor holds N*N entries");
    for row in 0..N {
        for column in 0..row {
            let offset = column * N + row;
            for previous in 0..column {
                factor[offset] -= factor[previous * N + row] * factor[previous * N + column];
            }
            factor[offset] /= factor[column * N + column];
        }
        let diagonal = row * N + row;
        for column in 0..row {
            let value = factor[column * N + row];
            factor[diagonal] -= value * value;
        }
        if factor[diagonal] <= 0.0 { return Err("Color-blend matrix is not positive definite."); }
        factor[diagonal] = factor[diagonal].sqrt();
    }
    Ok(())
}

/// The same factorization over a buffer the caller already holds: the matrix
/// becomes its factor in place, so a Schur system that lives in a retained slot
/// costs no copy. `factor_positive_matrix` wraps this, so there is one copy of
/// the arithmetic and the same operations run in the same order either way.
///
/// A fit's system is at most `MAX_FIT_COLORS` wide (a color can be constrained
/// once), so the sizes that actually occur are dispatched to the const-generic
/// form above; anything larger — only reachable through the public
/// `fit_prepared_blend` with a bigger palette — takes the same loop with a
/// runtime size, so the bound is a fast path, never a truncation.
fn factor_positive_matrix_in_place(factor: &mut [f64], size: usize) -> Result<(), &'static str> {
    match size {
        1 => factor_positive_const::<1>(factor),
        2 => factor_positive_const::<2>(factor),
        3 => factor_positive_const::<3>(factor),
        4 => factor_positive_const::<4>(factor),
        5 => factor_positive_const::<5>(factor),
        6 => factor_positive_const::<6>(factor),
        7 => factor_positive_const::<7>(factor),
        8 => factor_positive_const::<8>(factor),
        9 => factor_positive_const::<9>(factor),
        _ => factor_positive_generic(factor, size),
    }
}

/// The runtime-size factorization, kept for sizes the dispatch does not cover.
fn factor_positive_generic(factor: &mut [f64], size: usize) -> Result<(), &'static str> {
    for row in 0..size {
        for column in 0..row {
            let offset = column * size + row;
            for previous in 0..column {
                factor[offset] -= factor[previous * size + row] * factor[previous * size + column];
            }
            factor[offset] /= factor[column * size + column];
        }
        let diagonal = row * size + row;
        for column in 0..row {
            let value = factor[column * size + row];
            factor[diagonal] -= value * value;
        }
        if factor[diagonal] <= 0.0 { return Err("Color-blend matrix is not positive definite."); }
        factor[diagonal] = factor[diagonal].sqrt();
    }
    Ok(())
}

/// Forward/back substitution over an owned right-hand side: the caller's
/// buffer becomes the answer in place, so a RHS that is dead after the call
/// costs no second allocation. The arithmetic, and its order, is unchanged.
fn solve_factored(factor: &[f64], right: Vec<f64>, size: usize) -> Vec<f64> {
    let mut solved = right;
    solve_factored_in_place(factor, &mut solved, size);
    solved
}

/// The substitution with the system size known at compile time. The operations,
/// their order and every operand are the ones `solve_factored_in_place` runs for
/// the same `size`: only the row stride and the loop bounds become constants, so
/// the index arithmetic folds and the bounds checks on `factor`/`solved` are
/// resolved against the caller's slice once instead of per element.
fn solve_factored_const<const N: usize>(factor: &[f64], solved: &mut [f64]) {
    // One length test per call, not one per index. The two slices are narrowed
    // to exactly the entries the substitution reads, so every later index is
    // provably in range and the backend drops the per-element checks. The
    // arithmetic, its order and every operand are unchanged: the same
    // `factor[column * N + row]` and `solved[row]` values are read and written
    // in the same sequence.
    let factor = &factor[..N * N];
    let solved = &mut solved[..N];
    for row in 0..N {
        // Native forward substitution subtracts previous terms in reverse order.
        let mut value = solved[row];
        for column in (0..row).rev() { value -= solved[column] * factor[column * N + row]; }
        solved[row] = value / factor[row * N + row];
    }
    for row in (0..N).rev() {
        let mut value = solved[row];
        for column in row + 1..N { value -= solved[column] * factor[row * N + column]; }
        solved[row] = value / factor[row * N + row];
    }
}

/// The substitution itself, on a buffer the caller already holds. `solve_factored`
/// wraps this, so there is one copy of the arithmetic: a solved image that must
/// land in a retained buffer and one that must become an owned `Vec` go through
/// the same operations in the same order.
///
/// A fit's system is at most `MAX_FIT_COLORS` wide (a color can be constrained
/// once), so the sizes that actually occur are dispatched to the const-generic
/// form above; anything larger — only reachable through the public
/// `fit_prepared_blend` with a bigger palette — takes the same loop with a
/// runtime size, so the bound is a fast path, never a truncation.
fn solve_factored_in_place(factor: &[f64], solved: &mut [f64], size: usize) {
    match size {
        1 => solve_factored_const::<1>(factor, solved),
        2 => solve_factored_const::<2>(factor, solved),
        3 => solve_factored_const::<3>(factor, solved),
        4 => solve_factored_const::<4>(factor, solved),
        5 => solve_factored_const::<5>(factor, solved),
        6 => solve_factored_const::<6>(factor, solved),
        7 => solve_factored_const::<7>(factor, solved),
        8 => solve_factored_const::<8>(factor, solved),
        9 => solve_factored_const::<9>(factor, solved),
        _ => solve_factored_generic(factor, solved, size),
    }
}

/// The runtime-size substitution, kept for sizes the dispatch does not cover.
fn solve_factored_generic(factor: &[f64], solved: &mut [f64], size: usize) {
    for row in 0..size {
        // Native forward substitution subtracts previous terms in reverse order.
        let mut value = solved[row];
        for column in (0..row).rev() { value -= solved[column] * factor[column * size + row]; }
        solved[row] = value / factor[row * size + row];
    }
    for row in (0..size).rev() {
        let mut value = solved[row];
        for column in row + 1..size { value -= solved[column] * factor[row * size + column]; }
        solved[row] = value / factor[row * size + row];
    }
}

// Counts iterations that add a constraint so the solver tests exercise retained rows.
#[cfg(test)]
thread_local! {
    static ACTIVE_SET: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

#[cfg(test)]
fn note_active_set_iteration(added_constraint: bool) {
    ACTIVE_SET.with(|c| {
        let (iterations, added) = c.get();
        c.set((iterations + 1, added + added_constraint as usize));
    });
}

/// The palette-only part of `fit_multiple_colors`: the Gram matrix, its
/// Cholesky factor and the initial (all-ones constraint) solve. None of it
/// reads the pixel, so every fit against the same palette rebuilds the same
/// bits. `count` is the palette's entry count and `palette` its full contents
/// (checked by exact bit comparison, so a hash collision can only cost a miss,
/// never a different answer).
#[derive(Clone)]
struct PaletteSetup {
    palette: Vec<f32>,
    factor: Vec<f64>,
    solved_ones: Vec<f64>,
}

/// The palette's Gram matrix over its first three channels, filled the way the
/// retained nested loop fills it: every entry starts at `0f64`, accumulates the
/// three channel products in ascending channel order, and a diagonal entry adds
/// `0.01`. Both triangles of this matrix hold the same product of the same two
/// color scalars, and IEEE-754 multiplication of finite operands is
/// commutative bit for bit, so each off-diagonal product is computed once and
/// stored into both cells: the lower-triangular cell `gram[column * count + row]`
/// takes the value computed for the upper one, `gram[row * count + column]`.
/// The diagonal form is the same three products of a color with itself. No
/// accumulation order, operand or rounding changes; only the second evaluation
/// of an identical product is dropped.
///
/// A palette that holds a non-finite channel takes the original
/// one-computation-per-cell loop instead: NaN payload propagation through a
/// multiply depends on operand order on some machines, so those inputs keep
/// exactly the products — and every resulting bit — the nested loop produces.
/// The engine's own palettes are byte-derived and always finite; the general
/// form exists so the public `fit_prepared_blend` with an arbitrary float32
/// palette answers as it always did.
fn palette_gram(colors: &[f32], count: usize) -> Vec<f64> {
    let mut gram = vec![0f64; count * count];
    // The diagonal pass reads each color's three channels anyway, so the
    // finiteness test that selects the loop shape rides along with those loads
    // instead of scanning the palette a second time. Channel 3 never enters the
    // Gram, so it is not tested.
    let mut finite = true;
    for column in 0..count {
        let mut diagonal = 0f64;
        for channel in 0..3 {
            let value = colors[column * 4 + channel];
            finite &= value.is_finite();
            diagonal += value as f64 * value as f64;
        }
        gram[column * count + column] = diagonal + 0.01;
    }
    if finite {
        for column in 0..count {
            for row in column + 1..count {
                let mut value = 0f64;
                for channel in 0..3 { value += colors[row * 4 + channel] as f64 * colors[column * 4 + channel] as f64; }
                gram[column * count + row] = value;
                gram[row * count + column] = value;
            }
        }
    } else {
        for column in 0..count {
            for row in 0..count {
                let mut value = 0f64;
                for channel in 0..3 { value += colors[row * 4 + channel] as f64 * colors[column * 4 + channel] as f64; }
                gram[column * count + row] = value + if row == column { 0.01 } else { 0.0 };
            }
        }
    }
    gram
}

impl PaletteSetup {
    fn build(colors: &[f32]) -> Result<Self, &'static str> {
        let count = colors.len() / 4;
        let mut factor = palette_gram(colors, count);
        factor_positive_matrix_in_place(&mut factor, count)?;
        let solved_ones = solve_factored(&factor, vec![1f64; count], count);
        Ok(PaletteSetup { palette: colors.to_vec(), factor, solved_ones })
    }
}

/// A thread-local cache for the small palettes reused across pixels of a region.
struct SetupCache { slots: Vec<Option<PaletteSetup>> }

impl SetupCache {
    const BITS: u32 = 12;

    fn new() -> Self { SetupCache { slots: (0..1 << Self::BITS).map(|_| None).collect() } }

    fn slot(&self, colors: &[f32]) -> usize {
        let mut hash = 0x9e37_79b9_7f4a_7c15u64 ^ colors.len() as u64;
        for value in colors { hash = (hash ^ value.to_bits() as u64).wrapping_mul(0x0100_0000_01b3); hash ^= hash >> 29; }
        (hash >> (64 - Self::BITS)) as usize
    }

    /// The setup for this palette, built on a miss. A hit requires the same
    /// entry count and the same palette bits.
    fn get(&mut self, colors: &[f32]) -> Result<&PaletteSetup, &'static str> {
        let slot = self.slot(colors);
        let hit = self.slots[slot].as_ref().is_some_and(|setup| setup.palette.len() == colors.len()
            && setup.palette.iter().zip(colors).all(|(a, b)| a.to_bits() == b.to_bits()));
        if !hit { self.slots[slot] = Some(PaletteSetup::build(colors)?); }
        Ok(self.slots[slot].as_ref().unwrap())
    }
}

/// The largest palette a fit sees in practice: `collect_blend_indices` fills a
/// nine-entry buffer and every caller passes at most those nine colors, so a
/// fit's color count — and therefore its active-set row count, since a color
/// can be constrained once — is bounded by nine. The arena is sized from
/// `count`, not from this constant, so a larger palette (only reachable through
/// the public `fit_prepared_blend`) is served by the same code with a larger
/// arena: the bound is a starting capacity, never a truncation.
pub(crate) const MAX_FIT_COLORS: usize = 9;

/// Per-fit working buffers, reused across fits.
///
/// Every buffer here is dead once the fit returns: the answer is written into
/// storage the caller owns (the returned `BlendFit`'s own buffers, or a buffer
/// the caller reuses across a run of fits), so nothing here outlives the call.
/// The buffers can therefore be kept and refilled instead of allocated per fit.
/// Nothing borrows them past the call, and the fit calls no user code, so a
/// thread-local owner is enough.
///
/// The rows are slots in one flat arena, not a pool of `Vec`s. The active set
/// starts from the all-ones constraint and adds at most one constraint per
/// color, so a fit of `count` colors needs at most `count + 1` constraint rows
/// and as many solved rows, and slot `i` is `rows[i * count..][..count]`. A row
/// comes back by index, so there is no `pop`, no per-row `clear`/`resize` and no
/// growth branch; `begin` only sizes the arena, and every row is written before
/// it is read.
///
/// The arithmetic is untouched: each buffer is refilled with the same values in
/// the same order, so every accumulation sees the same operands in the same
/// sequence.
struct FitScratch {
    projection: Vec<f64>,
    constrained: Vec<bool>,
    /// `count + 1` constraint rows then `count + 1` solved rows, `count` long each.
    rows: Vec<f64>,
    /// The general branch's Schur system: `(count + 1) * (count + 1)` entries
    /// and `count + 1` right-hand sides, zeroed where they are used.
    schur: Vec<f64>,
    right: Vec<f64>,
}

impl Default for FitScratch {
    /// Allocate the arenas once at the practical bound, so a fit of nine colors
    /// or fewer never grows them.
    fn default() -> Self {
        FitScratch {
            projection: Vec::with_capacity(MAX_FIT_COLORS),
            constrained: Vec::with_capacity(MAX_FIT_COLORS),
            rows: vec![0.0; 2 * (MAX_FIT_COLORS + 1) * MAX_FIT_COLORS],
            schur: vec![0.0; (MAX_FIT_COLORS + 1) * (MAX_FIT_COLORS + 1)],
            right: vec![0.0; MAX_FIT_COLORS + 1],
        }
    }
}

impl FitScratch {
    /// Start a fit of `count` colors: size the two per-fit buffers and make sure
    /// every arena is at least as large as this fit needs. Growing is the only
    /// allocation, and it happens once per new largest palette.
    ///
    /// `projection` is only *sized* here, never filled: the fit writes every one
    /// of its `count` entries from a literal `0f64` before anything reads them,
    /// so a fill would be a write the fit overwrites unread. `constrained` is
    /// cleared here and only here — the fit sets flags as the active set grows,
    /// so this clear is what makes the next fit start from no constraints.
    fn begin(&mut self, count: usize) {
        self.projection.resize(count, 0.0);
        self.constrained.clear(); self.constrained.resize(count, false);
        grow(&mut self.rows, 2 * (count + 1) * count);
        grow(&mut self.schur, (count + 1) * (count + 1));
        grow(&mut self.right, count + 1);
    }
}

/// Make `buffer` at least `needed` long, keeping its contents and its
/// allocation. A buffer that is already long enough is left alone, so a run of
/// fits against the same palette sizes nothing.
fn grow(buffer: &mut Vec<f64>, needed: usize) {
    if buffer.len() < needed { buffer.resize(needed, 0.0); }
}

thread_local! {
    static FIT_SCRATCH: std::cell::RefCell<FitScratch> = std::cell::RefCell::new(FitScratch::default());
}

/// `fit_prepared_blend` writing into `out` instead of allocating a `BlendFit`.
/// The arithmetic, its order and every operand are the ones the owned form
/// runs; only the destination differs, so every bit of the answer is the same.
pub(crate) fn fit_prepared_blend_into(pixel: &[f64; 4], colors: &[f32], out: &mut BlendFit) -> Result<(), &'static str> {
    match colors.len() {
        4 => {
            out.color.clear(); out.color.extend_from_slice(&colors[..4]);
            out.weights.clear(); out.weights.push(1.0);
            out.self_weight = 1.0;
            Ok(())
        }
        8 => { fit_two_colors_into(pixel, colors, out); Ok(()) }
        _ => fit_multiple_colors_into(pixel, colors, out),
    }
}

fn fit_two_colors_into(pixel: &[f64; 4], colors: &[f32], out: &mut BlendFit) {
    let (color, weights, self_weight) = fit_two_colors_parts(pixel, colors);
    out.color.clear(); out.color.extend_from_slice(&color);
    out.weights.clear(); out.weights.extend_from_slice(&weights);
    out.self_weight = self_weight;
}

fn fit_multiple_colors_into(pixel: &[f64; 4], colors: &[f32], out: &mut BlendFit) -> Result<(), &'static str> {
    // Larger palettes are supported by the public API. Keep their quadratic
    // working storage local so a single large call cannot pin it on the thread.
    if colors.len() / 4 > MAX_FIT_COLORS {
        let setup = PaletteSetup::build(colors)?;
        return fit_with_setup(pixel, colors, &setup, &mut FitScratch::default(), out);
    }
    SETUPS.with(|c| fit_multiple_colors_into_with(pixel, colors, &mut c.borrow_mut(), out))
}

fn fit_multiple_colors_into_with(pixel: &[f64; 4], colors: &[f32], cache: &mut SetupCache, out: &mut BlendFit) -> Result<(), &'static str> {
    let setup = cache.get(colors)?;
    FIT_SCRATCH.with(|scratch| fit_with_setup(pixel, colors, setup, &mut scratch.borrow_mut(), out))
}

fn fit_with_setup(pixel: &[f64; 4], colors: &[f32], setup: &PaletteSetup, scratch: &mut FitScratch, out: &mut BlendFit) -> Result<(), &'static str> {
    let count = colors.len() / 4;
    let factor = &setup.factor;
    out.weights.resize(count, 0.0);
    scratch.begin(count);
    // Disjoint field borrows: the flat buffers stay inside the scratch (so
    // an early `?` return keeps their capacity), and the arena is split so a
    // constraint row and a solved row can be held at once.
    let FitScratch { projection, constrained, rows, schur, right } = &mut *scratch;
    let (constraint_rows, solved_rows) = rows.split_at_mut((count + 1) * count);
    let (color_nodes, _) = colors.as_chunks::<4>();
    // Each entry is written from a literal `0f64` before anything reads it,
    // so `begin` only has to size the buffer, not fill it. The accumulator's
    // start is the same `+0.0` the removed fill stored, and the channels are
    // added in the same order, so every sum is bit-identical: `0.0 + x` and
    // `x` agree for every finite `x`, and for `-0.0` both give `+0.0`.
    for (column, node) in projection.iter_mut().zip(color_nodes) {
        let mut value = 0f64;
        for channel in 0..3 { value += node[channel] as f64 * pixel[channel]; }
        *column = value;
    }
    // Slot 0 is the all-ones constraint and its prepared solve.
    constraint_rows[..count].fill(1.0);
    solved_rows[..count].copy_from_slice(&setup.solved_ones);
    let answer = &mut out.weights[..count];
    let mut n = 1usize;
    loop {
        'build: {
            // The first iteration's 1x1 Schur system, specialized: the same
            // operations in the same order as the general path below, without
            // the one-element system and the general factor/solve calls.
            // Most fits stop on this iteration, so it is the common case. A
            // non-finite Schur entry or right-hand side declines the fast path
            // (NaN signs are not stable across codegen), so the general code
            // below answers those exactly as it always did.
            if n == 1 {
                if let Some(result) = solve_first_iteration_scalar(&constraint_rows[..count], &solved_rows[..count], &projection[..count]) {
                    let multiplier = result?;
                    answer.copy_from_slice(&projection[..count]);
                    for color in 0..count { answer[color] += multiplier * constraint_rows[color]; }
                    break 'build;
                }
            }
            // The constraint and solved rows share one flat slot arena, so the
            // three accumulations below address a row once as a slice instead of
            // recomputing `row * count + color` and bounds-checking each load. A
            // row is `count` long, the same length as `projection`, so every zip
            // is exact.
            //
            // Every cell is written before it is read: the accumulation starts
            // from a literal `0f64` and stores the finished sum, so the arena's
            // previous contents never enter an expression. The zero-fill that
            // used to precede this loop was therefore dead — it wrote `n * n`
            // and `n` entries that the loop overwrote without reading. Starting
            // the accumulator at `0f64` is the same `+0.0` the fill stored, so
            // every sum is bit-identical: `0.0 + x` and `x` agree for every
            // finite `x`, and for `-0.0` both give `+0.0`.
            //
            // Only the lower triangle and the diagonal are built. The
            // factorization and both substitutions read `factor[column * n + row]`
            // with `row >= column` and nothing else — the forward pass reads
            // `column < row`, the back pass `column > row`, and the Cholesky
            // update reads `previous < column <= row` — so the cells with
            // `row < column` are never read by any consumer of this buffer. They
            // are left as the arena's previous contents, exactly as the removed
            // zero-fill left them, and no expression ever sees them. The cells
            // that are built keep their original operand order, accumulation
            // order and rounding, so every value the factorization reads is the
            // same bit it was.
            let schur = &mut schur[..n * n];
            let right = &mut right[..n];
            for column in 0..n {
                let solved_column = &solved_rows[column * count..][..count];
                for row in column..n {
                    let constraint_row = &constraint_rows[row * count..][..count];
                    let mut cell = 0f64;
                    for (constraint, solved) in constraint_row.iter().zip(solved_column) {
                        cell += constraint * solved;
                    }
                    schur[column * n + row] = cell;
                }
                let mut cell = 0f64;
                for (solved, projected) in solved_column.iter().zip(projection.iter()) {
                    cell += solved * projected;
                }
                right[column] = cell;
            }
            right[0] -= 1.0;
            factor_positive_matrix_in_place(schur, n)?;
            solve_factored_in_place(schur, right, n);
            answer.copy_from_slice(&projection[..count]);
            for constraint in 0..n {
                let multiplier = -right[constraint];
                let constraint_row = &constraint_rows[constraint * count..][..count];
                for (value, row) in answer.iter_mut().zip(constraint_row) {
                    *value += multiplier * row;
                }
            }
        }
        solve_factored_in_place(factor, answer, count);
        let (mut most_negative, mut minimum) = (usize::MAX, 0f64);
        for color in 0..count {
            if !constrained[color] && answer[color] < minimum { minimum = answer[color]; most_negative = color; }
        }
        #[cfg(test)]
        note_active_set_iteration(most_negative != usize::MAX && answer[most_negative] < -0.0001);
        if most_negative == usize::MAX || answer[most_negative] >= -0.0001 {
            // The answer is exactly `count` long, as the owned solve returned it.
            break;
        }
        constrained[most_negative] = true;
        // The new constraint row and its solved image take the next slots.
        // The row is zeroed first: the slot may hold an older fit's values.
        let row = &mut constraint_rows[n * count..][..count];
        row.fill(0.0);
        row[most_negative] = 1.0;
        let solved_row = &mut solved_rows[n * count..][..count];
        solved_row.copy_from_slice(row);
        solve_factored_in_place(factor, solved_row, count);
        n += 1;
    }
    out.color.clear();
    out.color.extend([pixel[0] as f32, pixel[1] as f32, pixel[2] as f32, pixel[3] as f32]);
    for channel in 0..3 {
        let mut value = 0f64;
        for (weight, node) in answer.iter().zip(color_nodes) { value += *weight * node[channel] as f64; }
        out.color[channel] = value as f32;
    }
    out.self_weight = answer[0];
    Ok(())
}

/// The owned form of the multi-color fit: the same arithmetic, with the answer
/// moved into a `BlendFit` the caller owns. The production entry points write
/// into a caller-owned buffer instead; this form is the tests' handle on the
/// same code.
#[cfg(test)]
fn fit_multiple_colors_with(pixel: &[f64; 4], colors: &[f32], cache: &mut SetupCache) -> Result<BlendFit, &'static str> {
    let mut out = BlendFit::default();
    fit_multiple_colors_into_with(pixel, colors, cache, &mut out)?;
    Ok(out)
}

/// The first active-set iteration's 1x1 Schur system, solved with the general
/// path's exact operations: the same accumulation order for the Schur entry
/// and the right-hand side, the same positive-definite check, the same `sqrt`,
/// and the same two sequential divisions the forward and back substitution
/// perform at size 1. Returns the negated multiplier the caller adds, as the
/// general path's `-multipliers[0]`.
///
/// `None` means "not this path": a non-finite Schur entry or right-hand side
/// declines, because NaN's sign bit is not specified by IEEE 754 and the two
/// loop shapes can differ there. The caller then runs the general code, which
/// answers those inputs exactly as it always did.
fn solve_first_iteration_scalar(constraint: &[f64], solved: &[f64], projection: &[f64]) -> Option<Result<f64, &'static str>> {
    let mut schur = 0f64;
    for color in 0..constraint.len() { schur += constraint[color] * solved[color]; }
    let mut right = 0f64;
    for color in 0..solved.len() { right += solved[color] * projection[color]; }
    right -= 1.0;
    if !schur.is_finite() || !right.is_finite() { return None; }
    if schur <= 0.0 { return Some(Err("Color-blend matrix is not positive definite.")); }
    let factor = schur.sqrt();
    let mut multiplier = right;
    multiplier /= factor;
    multiplier /= factor;
    Some(Ok(-multiplier))
}

#[cfg(test)]
mod scalar_first_iteration {
    //! The `n == 1` fast path must be the general path's arithmetic, bit for
    //! bit: the same 1x1 Schur entry, the same positive-definite check, the
    //! same `sqrt` and the same two sequential divisions of the forward and
    //! back substitution. The general path, run on a one-element system, is
    //! the oracle. Most fits stop on this iteration, so it is also the common
    //! case: `every_fit_runs_the_scalar_first_iteration` proves the branch is
    //! actually taken.
    use super::*;

    fn rng(seed: u64) -> impl FnMut() -> u64 {
        let mut state = seed | 1;
        move || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state }
    }

    /// The general path at `n == 1`: the reference's own loops over a 1x1
    /// system, returning the negated multiplier the caller adds.
    fn general_multiplier(constraint: &[f64], solved: &[f64], projection: &[f64]) -> Result<f64, &'static str> {
        let n = 1usize;
        let count = constraint.len();
        let mut schur = vec![0f64; n * n];
        let mut right = vec![0f64; n];
        for column in 0..n {
            for row in 0..n {
                for color in 0..count { schur[column * n + row] += constraint[color] * solved[color]; }
            }
            for color in 0..count { right[column] += solved[color] * projection[color]; }
        }
        right[0] -= 1.0;
        let multipliers = solve_factored_reference(&factor_positive_matrix(&schur, n)?, &right, n);
        Ok(-multipliers[0])
    }
    fn scalar(constraint: &[f64], solved: &[f64], projection: &[f64]) -> Result<Option<f64>, &'static str> {
        match solve_first_iteration_scalar(constraint, solved, projection) {
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(message)) => Err(message),
            None => Ok(None),
        }
    }

    #[test]
    fn scalar_schur_solve_matches_the_general_path_bit_for_bit() {
        let mut next = rng(0x5ca1_a000_0000_0001);
        let mut ok_cases = 0usize;
        let mut err_cases = 0usize;
        for count in 1..=9usize {
            for _case in 0..40 {
                // Real `solved_ones` entries are small positives; bias the
                // sweep the same way and keep a sign-varying tail, so both the
                // solving branch and the not-positive-definite error run.
                let solved: Vec<f64> = (0..count).map(|_| ((next() % 4001) as f64) / 1000.0).collect();
                // The first constraint is all ones, exactly as the fit builds it.
                let constraint = vec![1f64; count];
                let projection: Vec<f64> = (0..count).map(|_| ((next() % 2001) as f64) / 997.0).collect();
                match (scalar(&constraint, &solved, &projection), general_multiplier(&constraint, &solved, &projection)) {
                    (Ok(Some(a)), Ok(b)) => { assert_eq!(a.to_bits(), b.to_bits(), "multiplier differs at count={count}"); ok_cases += 1; }
                    (Err(a), Err(b)) => { assert_eq!(a, b); err_cases += 1; }
                    (Ok(None), _) => panic!("a finite system must not decline the fast path"),
                    (a, b) => panic!("success differs at count={count}: {a:?} vs {b:?}"),
                }
            }
        }
        // Degenerate finite Schur entries must fail with the general path's
        // error, before any division.
        for schur in [0.0f64, -0.0, -1e-300, -1.0] {
            let got = scalar(&[1.0], &[schur], &[3.0]);
            let want = general_multiplier(&[1.0], &[schur], &[3.0]);
            assert_eq!(got.unwrap_err(), want.unwrap_err(), "error differs for schur={schur}");
            err_cases += 1;
        }
        assert!(ok_cases >= 200, "the sweep must not thin out: {ok_cases}");
        assert!(err_cases > 0, "the negative-definite branch must be exercised");
    }

    /// Non-finite values are not the fast path's business: it declines, and the
    /// general code answers them. This test pins that division of labour, so a
    /// later "improvement" cannot quietly start guessing NaN signs.
    #[test]
    fn non_finite_first_iterations_decline_the_fast_path() {
        for special in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for target in 0..2 {
                let mut solved = [1.5f64, 2.5, 3.5];
                let mut projection = [4.5f64, 5.5, 6.5];
                if target == 0 { projection[1] = special; } else { solved[2] = special; }
                let constraint = [1.0f64; 3];
                assert!(scalar(&constraint, &solved, &projection).unwrap().is_none(),
                    "a non-finite system must decline for {special} at target {target}");
                // The general path still has an answer (or the positive-definite
                // error), unchanged by the fast path's existence.
                let _ = general_multiplier(&constraint, &solved, &projection);
            }
        }
    }

    /// The control: the fast path is the one the common fit takes. Every fit
    /// runs at least one iteration and that iteration is `n == 1`; this sweep
    /// also demands fits that stop there, since stopping on the first iteration
    /// is the case the specialization exists for.
    #[test]
    fn every_fit_runs_the_scalar_first_iteration_and_most_stop_there() {
        let mut next = rng(0x5ca1_a000_0000_0002);
        let mut cache = SetupCache::new();
        let mut fits = 0usize;
        let mut stopped_first = 0usize;
        for count in 3..=9usize {
            for _palette in 0..6 {
                let colors: Vec<f32> = (0..count * 4)
                    .map(|_| (next() % 256) as f64 as f32 * f32::from_bits((1.0f32 / 255.0).to_bits()))
                    .collect();
                for _pixel in 0..6 {
                    let mut pixel = [0.0f64; 4];
                    for channel in 0..4 { pixel[channel] = (next() % 256) as f64 / 255.0; }
                    ACTIVE_SET.with(|c| c.set((0, 0)));
                    let got = fit_multiple_colors_with(&pixel, &colors, &mut cache);
                    let want = fit_multiple_colors_reference(&pixel, &colors);
                    let (iterations, added) = ACTIVE_SET.with(|c| c.get());
                    match (got, want) {
                        (Ok(g), Ok(w)) => {
                            assert!(fit_fields_match(&g, &w), "fit differs for count={count}");
                            assert!(iterations >= 1);
                            fits += 1;
                            if iterations == 1 && added == 0 { stopped_first += 1; }
                        }
                        (Err(a), Err(b)) => assert_eq!(a, b),
                        _ => panic!("success differs for count={count}"),
                    }
                }
            }
        }
        assert!(fits >= 200, "the sweep must not thin out: {fits}");
        assert!(stopped_first > 0, "some fits must stop on the first iteration");
    }

    fn fit_fields_match(got: &BlendFit, want: &BlendFit) -> bool {
        got.color.len() == want.color.len() && got.weights.len() == want.weights.len()
            && got.color.iter().zip(&want.color).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.weights.iter().zip(&want.weights).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.self_weight.to_bits() == want.self_weight.to_bits()
    }
}

/// The original `solve_factored`, kept verbatim as the exact oracle: it copies
/// the borrowed right-hand side. Test-only.
#[cfg(test)]
fn solve_factored_reference(factor: &[f64], right: &[f64], size: usize) -> Vec<f64> {
    let mut solved = right.to_vec();
    for row in 0..size {
        // Native forward substitution subtracts previous terms in reverse order.
        let mut value = solved[row];
        for column in (0..row).rev() { value -= solved[column] * factor[column * size + row]; }
        solved[row] = value / factor[row * size + row];
    }
    for row in (0..size).rev() {
        let mut value = solved[row];
        for column in row + 1..size { value -= solved[column] * factor[row * size + column]; }
        solved[row] = value / factor[row * size + row];
    }
    solved
}

/// The original `fit_multiple_colors`, kept verbatim as the exact oracle: it
/// rebuilds the palette-only setup on every call. Test-only.
#[cfg(test)]
fn fit_multiple_colors_reference(pixel: &[f64; 4], colors: &[f32]) -> Result<BlendFit, &'static str> {
    let count = colors.len() / 4;
    let mut gram = vec![0f64; count * count];
    let mut projection = vec![0f64; count];
    for column in 0..count {
        for row in 0..count {
            let mut value = 0f64;
            for channel in 0..3 { value += colors[row * 4 + channel] as f64 * colors[column * 4 + channel] as f64; }
            gram[column * count + row] = value + if row == column { 0.01 } else { 0.0 };
        }
        for channel in 0..3 { projection[column] += colors[column * 4 + channel] as f64 * pixel[channel]; }
    }
    let factor = factor_positive_matrix(&gram, count)?;
    let mut constrained = vec![false; count];
    let mut constraints = vec![vec![1f64; count]];
    let mut solved_constraints = vec![solve_factored_reference(&factor, &constraints[0], count)];
    let weights = loop {
        let n = constraints.len();
        let mut schur = vec![0f64; n * n];
        let mut right = vec![0f64; n];
        for column in 0..n {
            for row in 0..n {
                for color in 0..count { schur[column * n + row] += constraints[row][color] * solved_constraints[column][color]; }
            }
            for color in 0..count { right[column] += solved_constraints[column][color] * projection[color]; }
        }
        right[0] -= 1.0;
        let multipliers = solve_factored_reference(&factor_positive_matrix(&schur, n)?, &right, n);
        let mut adjusted = projection.clone();
        for constraint in 0..n {
            let multiplier = -multipliers[constraint];
            for color in 0..count { adjusted[color] += multiplier * constraints[constraint][color]; }
        }
        let weights = solve_factored_reference(&factor, &adjusted, count);
        let (mut most_negative, mut minimum) = (usize::MAX, 0f64);
        for color in 0..count {
            if !constrained[color] && weights[color] < minimum { minimum = weights[color]; most_negative = color; }
        }
        if most_negative == usize::MAX || weights[most_negative] >= -0.0001 { break weights; }
        constrained[most_negative] = true;
        let mut constraint = vec![0f64; count];
        constraint[most_negative] = 1.0;
        solved_constraints.push(solve_factored_reference(&factor, &constraint, count));
        constraints.push(constraint);
    };
    let mut color: Vec<f32> = pixel.iter().map(|v| *v as f32).collect();
    for channel in 0..3 {
        let mut value = 0f64;
        for entry in 0..count { value += weights[entry] * colors[entry * 4 + channel] as f64; }
        color[channel] = value as f32;
    }
    let self_weight = weights[0];
    Ok(BlendFit { color, weights, self_weight })
}

/// Fit a pixel against 1, 2 or more prepared float32 BGRA colors.
pub fn fit_prepared_blend(pixel: &[f64; 4], colors: &[f32]) -> Result<BlendFit, &'static str> {
    let mut out = BlendFit::default();
    fit_prepared_blend_into(pixel, colors, &mut out)?;
    Ok(out)
}

thread_local! {
    static SETUPS: std::cell::RefCell<SetupCache> = std::cell::RefCell::new(SetupCache::new());
}

#[cfg(test)]
mod gram_symmetry {
    //! `palette_gram` against the retained nested loop, entry by entry, on the
    //! exact bits. The loop below is the original fill, kept verbatim as the
    //! oracle: same `0f64` start, same ascending channel order, same `0.01` on
    //! the diagonal.
    use super::*;

    fn rng(seed: u64) -> impl FnMut() -> u64 {
        let mut state = seed | 1;
        move || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state }
    }

    fn reference_gram(colors: &[f32], count: usize) -> Vec<f64> {
        let mut gram = vec![0f64; count * count];
        for column in 0..count {
            for row in 0..count {
                let mut value = 0f64;
                for channel in 0..3 { value += colors[row * 4 + channel] as f64 * colors[column * 4 + channel] as f64; }
                gram[column * count + row] = value + if row == column { 0.01 } else { 0.0 };
            }
        }
        gram
    }

    fn bits(values: &[f64]) -> Vec<u64> { values.iter().map(|v| v.to_bits()).collect() }

    /// Every entry, every size the engine can see plus sizes past the practical
    /// bound, on byte-derived palettes: the exact bits must match the loop.
    #[test]
    fn the_symmetric_fill_matches_the_nested_loop_on_every_entry() {
        let mut next = rng(0x5eed_0f17_0000_0001);
        let mut checked = 0usize;
        for count in 1..=12usize {
            for _palette in 0..6 {
                let colors: Vec<f32> = (0..count * 4)
                    .map(|_| (next() % 256) as f64 as f32 * f32::from_bits((1.0f32 / 255.0).to_bits()))
                    .collect();
                let got = palette_gram(&colors, count);
                let want = reference_gram(&colors, count);
                assert_eq!(bits(&got), bits(&want), "gram differs for count={count}");
                checked += 1;
            }
        }
        assert!(checked >= 60, "the sweep must not thin out: {checked}");
    }

    /// Structured and degenerate palettes: all-zero colors, repeated colors,
    /// one channel varying, and the extremes of the f32 range.
    #[test]
    fn degenerate_palettes_keep_the_same_bits() {
        let mut palettes: Vec<Vec<f32>> = Vec::new();
        palettes.push(vec![0f32; 4 * 5]);
        palettes.push(vec![1f32; 4 * 4]);
        palettes.push((0..4 * 6).map(|i| if i % 4 < 3 { (i / 4) as f32 } else { 0.0 }).collect());
        palettes.push((0..4 * 7).map(|i| if i % 4 == 0 { f32::MAX } else { 0.0 }).collect());
        palettes.push((0..4 * 3).map(|i| if i % 4 == 1 { f32::MIN_POSITIVE } else { 0.0 }).collect());
        palettes.push((0..4 * 9).map(|i| if i % 4 == 2 { -0.0f32 } else { 0.5 }).collect());
        for colors in palettes {
            let count = colors.len() / 4;
            assert_eq!(bits(&palette_gram(&colors, count)), bits(&reference_gram(&colors, count)),
                "gram differs for a degenerate palette of {count} colors");
        }
    }

    /// Non-finite channels take the original loop, so NaN payloads and signs
    /// are the ones the nested loop produces.
    #[test]
    fn non_finite_palettes_take_the_original_loop() {
        let mut next = rng(0xdead_0f17_0000_0002);
        for count in 1..=6usize {
            let mut colors: Vec<f32> = (0..count * 4).map(|_| (next() % 256) as f32 / 255.0).collect();
            let len = colors.len();
            let (nan_at, inf_at, neg_at) = ((next() as usize) % len, (next() as usize) % len, (next() as usize) % len);
            colors[nan_at] = f32::NAN;
            colors[inf_at] = f32::INFINITY;
            colors[neg_at] = f32::NEG_INFINITY;
            assert_eq!(bits(&palette_gram(&colors, count)), bits(&reference_gram(&colors, count)),
                "gram differs for a non-finite palette of {count} colors");
        }
    }

    /// The control: a fill that mirrors the wrong cell, or that adds the
    /// diagonal term to an off-diagonal cell, is caught by the comparison.
    #[test]
    fn a_mis_mirrored_fill_is_caught() {
        let colors: Vec<f32> = (0..4 * 5).map(|i| (i as f32) * 0.25).collect();
        let count = colors.len() / 4;
        let want = bits(&reference_gram(&colors, count));
        // Wrong cell: the mirror write lands on the transposed index.
        let mut wrong = palette_gram(&colors, count);
        let mirrored = wrong[2 * count + 1];
        wrong[1 * count + 2] = mirrored + 1.0;
        assert_ne!(bits(&wrong), want, "a wrong mirror cell must be visible");
        // Wrong diagonal: the `0.01` added to an off-diagonal entry.
        let mut wrong = palette_gram(&colors, count);
        wrong[0 * count + 1] += 0.01;
        assert_ne!(bits(&wrong), want, "a stray diagonal term must be visible");
    }
}

#[cfg(test)]
mod setup_equivalence {
    //! The cached production `fit_multiple_colors` against the retained,
    //! verbatim original that rebuilds the palette-only setup on every call.
    //! Every field the caller can observe is compared exactly: the fitted color
    //! and every weight by `to_bits()`, and `self_weight`. A cache that ever
    //! served a different palette's factor would fail here.
    use super::*;

    fn rng(seed: u64) -> impl FnMut() -> u64 {
        let mut state = seed | 1;
        move || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state }
    }

    fn same(got: &BlendFit, want: &BlendFit) -> bool {
        got.color.len() == want.color.len() && got.weights.len() == want.weights.len()
            && got.color.iter().zip(&want.color).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.weights.iter().zip(&want.weights).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.self_weight.to_bits() == want.self_weight.to_bits()
    }

    #[test]
    fn cached_setup_matches_the_retained_rebuild_on_every_field() {
        let mut next = rng(0x1234_5678_9abc_def1);
        let mut checked = 0usize;
        let mut cache = SetupCache::new();
        for count in 3..=9usize {
            for _palette in 0..8 {
                let colors: Vec<f32> = (0..count * 4)
                    .map(|_| (next() % 256) as f64 as f32 * f32::from_bits((1.0f32 / 255.0).to_bits()))
                    .collect();
                // Many pixels against one palette: the first fits miss, the rest hit.
                for _pixel in 0..7 {
                    let mut pixel = [0.0f64; 4];
                    for channel in 0..4 { pixel[channel] = (next() % 256) as f64 / 255.0; }
                    let got = fit_multiple_colors_with(&pixel, &colors, &mut cache);
                    let want = fit_multiple_colors_reference(&pixel, &colors);
                    match (got, want) {
                        (Ok(g), Ok(w)) => { assert!(same(&g, &w), "fit differs for count={count}"); checked += 1; }
                        (Err(a), Err(b)) => assert_eq!(a, b),
                        _ => panic!("success differs for count={count}"),
                    }
                }
            }
        }
        assert!(checked >= 300, "the sweep must not thin out: {checked}");
    }

    /// The control: a cache that keys only on the entry count serves another
    /// palette's factor, and this test proves the comparison above catches it.
    #[test]
    fn a_count_only_key_would_serve_the_wrong_setup() {
        let mut next = rng(0xfeed_face_0000_0001);
        let colors_a: Vec<f32> = (0..20).map(|_| (next() % 256) as f32 / 255.0).collect();
        let colors_b: Vec<f32> = (0..20).map(|_| (next() % 256) as f32 / 255.0).collect();
        let mut pixel = [0.0f64; 4];
        for channel in 0..4 { pixel[channel] = (next() % 256) as f64 / 255.0; }
        let mut cache = SetupCache::new();
        let first = fit_multiple_colors_with(&pixel, &colors_a, &mut cache).expect("first");
        let right = fit_multiple_colors_reference(&pixel, &colors_a).expect("reference");
        assert!(same(&first, &right), "the cache's own answer is correct");
        let count = colors_a.len() / 4;
        let slot = cache.slot(&colors_a);
        // Plant another palette's setup in the slot this palette actually uses.
        cache.slots[slot] = Some(PaletteSetup::build(&colors_b).expect("stale"));
        let poisoned = fit_multiple_colors_with(&pixel, &colors_a, &mut cache).expect("poisoned");
        assert!(same(&poisoned, &right), "a whole-palette check must reject the planted setup");
        // The count-only comparator really would have accepted it, so the bits
        // are what reject it.
        let only_count = cache.slots[slot].as_ref().is_some_and(|s| s.palette.len() / 4 == count);
        assert!(only_count, "the stale entry has the same entry count, so only the bits reject it");
    }
}

#[cfg(test)]
mod owned_rhs_equivalence {
    //! `solve_factored` now consumes an owned right-hand side and solves in
    //! place. These tests pin the two properties that make that safe: the
    //! answer is bit-identical to the retained copying solver for every input
    //! (including zeros, size 1 and non-finite values), and the returned buffer
    //! is the caller's own allocation, so no copy happened.
    use super::*;

    fn rng(seed: u64) -> impl FnMut() -> u64 {
        let mut state = seed | 1;
        move || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state }
    }

    fn bits(values: &[f64]) -> Vec<u64> { values.iter().map(|v| v.to_bits()).collect() }

    /// A symmetric positive-definite matrix: `A = L L^T + n I` with a strictly
    /// positive diagonal, so `factor_positive_matrix` always succeeds.
    fn positive_definite(next: &mut impl FnMut() -> u64, size: usize) -> Vec<f64> {
        let mut lower = vec![0f64; size * size];
        for column in 0..size {
            for row in column..size {
                lower[column * size + row] = ((next() % 2001) as f64 - 1000.0) / 1000.0;
            }
            lower[column * size + column] = 1.0 + (next() % 1000) as f64 / 1000.0;
        }
        let mut matrix = vec![0f64; size * size];
        for column in 0..size {
            for row in 0..size {
                let mut value = 0f64;
                for inner in 0..size { value += lower[inner * size + row] * lower[inner * size + column]; }
                matrix[column * size + row] = value + if row == column { size as f64 } else { 0.0 };
            }
        }
        matrix
    }

    #[test]
    fn owned_solve_matches_the_copying_solver_bit_for_bit() {
        let mut next = rng(0x0dd_ba11_5eed_0001);
        let mut checked = 0usize;
        for size in 1..=9usize {
            for _case in 0..24 {
                let matrix = positive_definite(&mut next, size);
                let factor = factor_positive_matrix(&matrix, size).expect("positive definite");
                let mut right: Vec<f64> = (0..size).map(|_| ((next() % 4001) as f64 - 2000.0) / 1000.0).collect();
                // Zeros, signed zeros and a repeated value are ordinary inputs.
                if size > 1 { right[0] = 0.0; right[size - 1] = -0.0; }
                let want = solve_factored_reference(&factor, &right, size);
                let owned = right.clone();
                let pointer = owned.as_ptr();
                let got = solve_factored(&factor, owned, size);
                assert_eq!(bits(&got), bits(&want), "solve differs at size {size}");
                assert_eq!(got.as_ptr(), pointer, "the solve must reuse the caller's buffer");
                checked += 1;
            }
        }
        assert!(checked >= 200, "the sweep must not thin out: {checked}");
    }

    #[test]
    fn owned_solve_propagates_non_finite_inputs_identically() {
        let mut next = rng(0x0dd_ba11_5eed_0002);
        let size = 4usize;
        let matrix = positive_definite(&mut next, size);
        let factor = factor_positive_matrix(&matrix, size).expect("positive definite");
        for special in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut right: Vec<f64> = (0..size).map(|_| (next() % 100) as f64 / 100.0).collect();
            right[2] = special;
            let want = solve_factored_reference(&factor, &right, size);
            let got = solve_factored(&factor, right, size);
            assert_eq!(bits(&got), bits(&want), "non-finite propagation differs for {special}");
        }
    }

    /// The const-generic dispatch must run the same arithmetic as the retained
    /// runtime-size solver, bit for bit, at every size the dispatch covers and
    /// at a size it does not (which must take the generic fallback and agree
    /// too). Extreme magnitudes and signed zeros are included: a re-associated
    /// subtraction would show up there first.
    #[test]
    fn the_const_dispatch_matches_the_runtime_solver_at_every_size() {
        let mut next = rng(0x0dd_ba11_5eed_0004);
        let mut checked = 0usize;
        for size in [1usize, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 17] {
            for case in 0..8 {
                let matrix = positive_definite(&mut next, size);
                let factor = factor_positive_matrix(&matrix, size).expect("positive definite");
                let mut right: Vec<f64> = (0..size)
                    .map(|_| ((next() % 4000_001) as f64 - 2_000_000.0) / 1000.0)
                    .collect();
                match case {
                    0 => { right[0] = 0.0; }
                    1 => { right[size - 1] = -0.0; }
                    2 => { for value in right.iter_mut() { *value *= 1e150; } }
                    3 => { for value in right.iter_mut() { *value *= 1e-150; } }
                    4 => { right[size / 2] = f64::MIN_POSITIVE; }
                    5 => { for value in right.iter_mut().step_by(2) { *value = -*value; } }
                    _ => {}
                }
                let mut got = right.clone();
                solve_factored_in_place(&factor, &mut got, size);
                let mut generic = right.clone();
                solve_factored_generic(&factor, &mut generic, size);
                let want = solve_factored_reference(&factor, &right, size);
                assert_eq!(bits(&got), bits(&want), "dispatched solve differs at size {size}, case {case}");
                assert_eq!(bits(&generic), bits(&want), "generic solve differs at size {size}, case {case}");
                checked += 1;
            }
        }
        assert!(checked >= 90, "the sweep must not thin out: {checked}");
    }

    /// The factorization and both substitutions read only `factor[column * size + row]`
    /// with `row >= column`: the forward pass reads `column < row`, the back pass
    /// `column > row`, and the Cholesky update reads `previous < column <= row`.
    /// The cells with `row < column` are therefore never read, which is what lets
    /// the Schur build skip them. This is the negative control: poison exactly
    /// those cells with a NaN and demand the same factor bits and the same solved
    /// bits as the untouched matrix. A read of any of them would surface as a NaN.
    ///
    /// The factor writes only the lower triangle and the diagonal too, so the
    /// comparison is over those cells: the poisoned upper triangle is left as the
    /// NaN it was, in both buffers, and is not part of the factor.
    #[test]
    fn the_factor_and_solve_never_read_the_upper_triangle() {
        let mut next = rng(0x0dd_ba11_5eed_0009);
        let mut checked = 0usize;
        for size in 1..=MAX_FIT_COLORS {
            for _case in 0..8 {
                let matrix = positive_definite(&mut next, size);
                let mut poisoned = matrix.clone();
                for column in 0..size {
                    for row in 0..column { poisoned[column * size + row] = f64::NAN; }
                }
                let want = factor_positive_matrix(&matrix, size).expect("positive definite");
                let got = factor_positive_matrix(&poisoned, size).expect("positive definite");
                for column in 0..size {
                    for row in column..size {
                        let index = column * size + row;
                        assert_eq!(got[index].to_bits(), want[index].to_bits(),
                            "the factor read an upper-triangle cell at size {size}, ({column},{row})");
                    }
                }
                let right: Vec<f64> = (0..size).map(|_| ((next() % 4000_001) as f64 - 2_000_000.0) / 1000.0).collect();
                let mut solved_want = right.clone();
                solve_factored_in_place(&want, &mut solved_want, size);
                let mut solved_got = right.clone();
                solve_factored_in_place(&got, &mut solved_got, size);
                assert_eq!(bits(&solved_got), bits(&solved_want), "the solve read an upper-triangle cell at size {size}");
                checked += 1;
            }
        }
        assert!(checked >= 70, "the sweep must not thin out: {checked}");
    }

    /// The dispatch must actually be taken for the sizes a fit reaches, and the
    /// fallback for a larger one: compare the dispatched answer with the
    /// generic answer directly so a silent `_` arm cannot pass unnoticed.
    #[test]
    fn the_fit_sizes_take_the_const_arm() {
        let mut next = rng(0x0dd_ba11_5eed_0005);
        for size in 1..=MAX_FIT_COLORS {
            let matrix = positive_definite(&mut next, size);
            let factor = factor_positive_matrix(&matrix, size).expect("positive definite");
            let right: Vec<f64> = (0..size).map(|_| (next() % 97) as f64 / 7.0).collect();
            let mut dispatched = right.clone();
            solve_factored_in_place(&factor, &mut dispatched, size);
            let mut generic = right.clone();
            solve_factored_generic(&factor, &mut generic, size);
            assert_eq!(bits(&dispatched), bits(&generic), "size {size}");
        }
    }

    /// The slice narrowing must not turn a short buffer into a silent read.
    /// `solve_factored_const` now narrows `factor` to `N*N` and `solved` to `N`
    /// at the top, so a caller that passes less must panic there rather than
    /// read past the end. This is the negative control for the hoist: it pins
    /// that the length test still exists, and that it fires.
    #[test]
    fn a_short_solve_buffer_panics_instead_of_reading_past_the_end() {
        let mut next = rng(0x0dd_ba11_5eed_0008);
        for size in 1..=MAX_FIT_COLORS {
            let matrix = positive_definite(&mut next, size);
            let factor = factor_positive_matrix(&matrix, size).expect("positive definite");
            // A factor one entry short of N*N.
            let short_factor = &factor[..size * size - 1];
            let mut right = vec![1.0f64; size];
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                solve_factored_in_place(short_factor, &mut right, size);
            }));
            assert!(caught.is_err(), "a short factor must panic at size {size}");
            // A right-hand side one entry short of N.
            let mut short_right = vec![1.0f64; size - 1];
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                solve_factored_in_place(&factor, &mut short_right, size);
            }));
            assert!(caught.is_err(), "a short right-hand side must panic at size {size}");
        }
    }

    /// One matrix's factor, both ways, against the retained runtime-size oracle:
    /// the factor's bits (or the error message) must agree, dispatched and
    /// generic. NaN/infinity are compared by bit pattern too, so a re-associated
    /// subtraction or a moved `sqrt` shows up here.
    fn check_factor_at(matrix: &[f64], size: usize) {
        let want = factor_positive_matrix(matrix, size).map(|f| bits(&f));
        let mut dispatched = matrix.to_vec();
        let got = factor_positive_matrix_in_place(&mut dispatched, size).map(|_| bits(&dispatched));
        assert_eq!(got, want, "dispatched factor differs at size {size}");
        let mut generic = matrix.to_vec();
        let got = factor_positive_generic(&mut generic, size).map(|_| bits(&generic));
        assert_eq!(got, want, "generic factor differs at size {size}");
    }

    /// The const dispatch must reproduce the runtime-size factorization's bits
    /// — and its accept/reject decision — at every size it covers and at sizes
    /// it does not (which must take the generic fallback and agree too).
    #[test]
    fn the_const_factor_matches_the_runtime_factor_at_every_size() {
        let mut next = rng(0x0dd_ba11_5eed_0006);
        let mut checked = 0usize;
        for size in [1usize, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 17] {
            for case in 0..8 {
                let mut matrix = positive_definite(&mut next, size);
                match case {
                    // Zero and signed-zero entries: the diagonal is then zero or
                    // negative, so the factorization must be refused identically.
                    1 => { matrix.fill(0.0); }
                    2 => { for value in matrix.iter_mut() { *value = -*value; } }
                    // Extreme magnitudes with a positive diagonal.
                    3 => {
                        matrix.fill(0.0);
                        for index in 0..size { matrix[index * size + index] = 1e150; }
                    }
                    4 => {
                        matrix.fill(0.0);
                        for index in 0..size { matrix[index * size + index] = 1e-150; }
                    }
                    5 => {
                        matrix.fill(0.0);
                        for index in 0..size { matrix[index * size + index] = f64::MIN_POSITIVE; }
                    }
                    // Alternating signs off the diagonal, positive diagonal.
                    6 => {
                        for column in 0..size {
                            for row in 0..size {
                                if row != column { matrix[column * size + row] = -matrix[column * size + row]; }
                            }
                        }
                    }
                    // Non-finite entries: rejected or propagated, but identically.
                    7 => { matrix[size * size - 1] = f64::NAN; }
                    _ => {}
                }
                check_factor_at(&matrix, size);
                checked += 1;
            }
        }
        assert!(checked >= 96, "the sweep must not thin out: {checked}");
    }

    /// The dispatch must actually be taken for the sizes a fit reaches, and the
    /// generic arm must be what a larger size takes: compare the dispatched
    /// factor with the generic one directly so a silent `_` arm cannot pass
    /// unnoticed.
    #[test]
    fn the_factor_sizes_take_the_const_arm() {
        let mut next = rng(0x0dd_ba11_5eed_0007);
        for size in 1..=MAX_FIT_COLORS {
            let matrix = positive_definite(&mut next, size);
            let mut dispatched = matrix.clone();
            factor_positive_matrix_in_place(&mut dispatched, size).expect("positive definite");
            let mut generic = matrix.clone();
            factor_positive_generic(&mut generic, size).expect("positive definite");
            assert_eq!(bits(&dispatched), bits(&generic), "factor differs at size {size}");
        }
        // Beyond the fit's bound the dispatch must fall through to the same loop.
        let size = MAX_FIT_COLORS + 3;
        let matrix = positive_definite(&mut next, size);
        let mut dispatched = matrix.clone();
        factor_positive_matrix_in_place(&mut dispatched, size).expect("positive definite");
        let mut generic = matrix.clone();
        factor_positive_generic(&mut generic, size).expect("positive definite");
        assert_eq!(bits(&dispatched), bits(&generic), "the fallback must be the generic loop");
    }

    /// The production fit must still match the retained oracle exactly, and the
    /// retained-constraint path (the one RHS that keeps its copy) must actually
    /// run: at least one fit in the sweep adds a constraint.
    #[test]
    fn production_fit_with_added_constraints_matches_the_oracle() {
        let mut next = rng(0x0dd_ba11_5eed_0003);
        let mut cache = SetupCache::new();
        let mut fits = 0usize;
        let mut added_total = 0usize;
        for count in 3..=9usize {
            for _palette in 0..6 {
                let colors: Vec<f32> = (0..count * 4)
                    .map(|_| (next() % 256) as f64 as f32 * f32::from_bits((1.0f32 / 255.0).to_bits()))
                    .collect();
                for _pixel in 0..6 {
                    let mut pixel = [0.0f64; 4];
                    for channel in 0..4 { pixel[channel] = (next() % 256) as f64 / 255.0; }
                    ACTIVE_SET.with(|c| c.set((0, 0)));
                    let got = fit_multiple_colors_with(&pixel, &colors, &mut cache);
                    let (iterations, added) = ACTIVE_SET.with(|c| c.get());
                    let want = fit_multiple_colors_reference(&pixel, &colors);
                    match (got, want) {
                        (Ok(g), Ok(w)) => {
                            assert!(same_fit(&g, &w), "fit differs for count={count}");
                            assert!(iterations >= 1, "every fit runs at least one iteration");
                            fits += 1;
                            added_total += added;
                        }
                        (Err(a), Err(b)) => assert_eq!(a, b),
                        _ => panic!("success differs for count={count}"),
                    }
                }
            }
        }
        assert!(fits >= 200, "the sweep must not thin out: {fits}");
        assert!(added_total > 0, "the retained-constraint path must be exercised");
    }

    fn same_fit(got: &BlendFit, want: &BlendFit) -> bool {
        got.color.len() == want.color.len() && got.weights.len() == want.weights.len()
            && got.color.iter().zip(&want.color).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.weights.iter().zip(&want.weights).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.self_weight.to_bits() == want.self_weight.to_bits()
    }
}

#[cfg(test)]
mod fit_scratch_ownership {
    //! The fit's working buffers are retained in a thread-local `FitScratch`
    //! and refilled per fit. These tests pin the ownership properties that make
    //! that safe, one per property:
    //!
    //! - every buffer is sized to exactly `count` before use, and the one buffer
    //!   whose contents the fit does not write first (`constrained`) is cleared,
    //!   so no stale value and no uninitialised tail is ever read;
    //! - the capacity is genuinely reused across fits (same allocation, no
    //!   reallocation) and a re-entered fit starts from no constraints;
    //! - the borrow is released when the fit returns, so the next fit on this
    //!   thread can take it (a live borrow would panic) and another thread gets
    //!   its own scratch;
    //! - no two live rows share one allocation, in either loop branch, and the
    //!   returned answer never aliases a scratch row;
    //! - the answer stays bit-identical to the retained oracle across repeated,
    //!   interleaved and non-finite calls.
    use super::*;

    fn rng(seed: u64) -> impl FnMut() -> u64 {
        let mut state = seed | 1;
        move || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state }
    }

    fn palette(next: &mut impl FnMut() -> u64, count: usize) -> Vec<f32> {
        (0..count * 4).map(|_| (next() % 256) as f64 as f32 * f32::from_bits((1.0f32 / 255.0).to_bits())).collect()
    }

    fn pixel(next: &mut impl FnMut() -> u64) -> [f64; 4] {
        let mut pixel = [0.0f64; 4];
        for channel in 0..4 { pixel[channel] = (next() % 256) as f64 / 255.0; }
        pixel
    }

    fn same_fit(got: &BlendFit, want: &BlendFit) -> bool {
        got.color.len() == want.color.len() && got.weights.len() == want.weights.len()
            && got.color.iter().zip(&want.color).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.weights.iter().zip(&want.weights).all(|(a, b)| a.to_bits() == b.to_bits())
            && got.self_weight.to_bits() == want.self_weight.to_bits()
    }

    /// `begin` is the only place a buffer is sized, and it must leave both
    /// per-fit buffers exactly `count` long, with every arena at least as large
    /// as this fit can need. `constrained` is cleared here and only here, so it
    /// must come back all-false. `projection` is *not* filled: the fit writes
    /// every entry from a literal `0f64` before reading it, so `begin` leaves
    /// the previous fit's values in place and the next test pins that they
    /// cannot reach an answer. The row slots are not cleared here either: the
    /// fit writes each row before it reads it.
    #[test]
    fn begin_sizes_every_buffer_and_clears_the_constraint_flags() {
        FIT_SCRATCH.with(|scratch| {
            let mut scratch = scratch.borrow_mut();
            // Leave the scratch in a dirty, oversized state, as a previous fit
            // of a larger palette would.
            scratch.projection = vec![7.5; 9];
            scratch.constrained = vec![true; 9];
            scratch.rows = vec![3.5; 2 * 10 * 9];
            scratch.schur = vec![4.5; 10 * 10];
            scratch.right = vec![5.5; 10];
            scratch.begin(3);
            assert_eq!(scratch.projection.len(), 3, "projection must be exactly count long");
            assert_eq!(scratch.constrained.len(), 3, "constrained must be exactly count long");
            assert!(scratch.constrained.iter().all(|&v| !v), "constrained must be cleared");
            // The arenas keep their allocation and are never shorter than the
            // largest fit that has run: a smaller fit must not shrink them.
            assert!(scratch.rows.len() >= 2 * (3 + 1) * 3, "the row arena must hold count + 1 rows each side");
            assert!(scratch.schur.len() >= (3 + 1) * (3 + 1), "the Schur arena must hold a count + 1 system");
            assert!(scratch.right.len() >= 3 + 1, "the right-hand side arena must hold count + 1 entries");
        });
    }

    /// The removed `projection` fill is dead: the fit writes every entry from a
    /// literal `0f64` before anything reads it, so a buffer poisoned with values
    /// that would change any sum (a huge finite value, a NaN, a negative zero)
    /// must still produce the oracle's exact bits. This is the negative control
    /// for the fill's removal: if a later change ever read an entry before
    /// writing it, the poison would show up in the answer.
    #[test]
    fn a_poisoned_projection_buffer_cannot_reach_the_answer() {
        let mut next = rng(0x5c2a_7c4e_0000_0007);
        let mut cache = SetupCache::new();
        let poisons = [1e300f64, f64::NAN, -0.0, f64::INFINITY, -1e300];
        for count in 1..=MAX_FIT_COLORS {
            for (index, poison) in poisons.iter().enumerate() {
                let colors = palette(&mut next, count);
                let sample = pixel(&mut next);
                FIT_SCRATCH.with(|scratch| {
                    let mut scratch = scratch.borrow_mut();
                    scratch.projection.clear();
                    scratch.projection.resize(count, *poison);
                    // A stale flag would also change the active set, so poison
                    // that too: `begin` must clear it.
                    scratch.constrained.clear();
                    scratch.constrained.resize(count, index % 2 == 0);
                });
                let got = fit_multiple_colors_with(&sample, &colors, &mut cache).expect("fit");
                let want = fit_multiple_colors_reference(&sample, &colors).expect("oracle");
                assert!(same_fit(&got, &want),
                    "a poisoned projection/constrained buffer changed the answer (count={count}, poison={poison:?})");
            }
        }
    }

    /// A row slot is written before it is read: the fit zeroes the slot it takes
    /// and the arena is the same allocation across fits, so a refill costs no
    /// allocation and no stale value from an earlier fit can survive.
    #[test]
    fn a_row_slot_is_zeroed_before_use_and_the_arena_is_reused() {
        let mut next = rng(0x5c2a_7c4e_0000_0006);
        let mut cache = SetupCache::new();
        // A nine-color fit first, so the arena is at its largest and every slot
        // holds that fit's values.
        let large = palette(&mut next, 9);
        let _ = fit_multiple_colors_with(&pixel(&mut next), &large, &mut cache);
        let (pointer, length) = FIT_SCRATCH.with(|scratch| {
            let scratch = scratch.borrow();
            (scratch.rows.as_ptr(), scratch.rows.len())
        });
        // A three-color fit reuses the same arena and must not read the nine
        // color values the slots still hold.
        let small = palette(&mut next, 3);
        let small_pixel = pixel(&mut next);
        let got = fit_multiple_colors_with(&small_pixel, &small, &mut cache).expect("fit");
        let want = fit_multiple_colors_reference(&small_pixel, &small).expect("oracle");
        assert!(same_fit(&got, &want), "a refilled fit must match the oracle");
        FIT_SCRATCH.with(|scratch| {
            let scratch = scratch.borrow();
            assert_eq!(scratch.rows.as_ptr(), pointer, "the row arena must be the same allocation");
            assert_eq!(scratch.rows.len(), length, "a smaller fit must not shrink the arena");
            // The all-ones constraint the fit just built is exactly count long
            // and all ones, so the slot was rewritten, not inherited.
            assert_eq!(&scratch.rows[..3], &[1.0, 1.0, 1.0], "the constraint slot must be freshly written");
        });
    }

    /// The borrow must not outlive the fit: a live borrow would make the next
    /// fit panic, and a leaked one would be a use-after-free hazard for the
    /// thread-local. `try_borrow_mut` succeeding is the proof it was released.
    #[test]
    fn the_scratch_borrow_is_released_when_the_fit_returns() {
        let mut next = rng(0x5c2a_7c4e_0000_0001);
        let mut cache = SetupCache::new();
        for count in 3..=9usize {
            let colors = palette(&mut next, count);
            let sample = pixel(&mut next);
            let _ = fit_multiple_colors_with(&sample, &colors, &mut cache);
            FIT_SCRATCH.with(|scratch| {
                let _guard = scratch.try_borrow_mut().expect("the fit left its scratch borrow live");
            });
        }
    }

    /// Capacity is reused across fits: a fit of a smaller palette after a
    /// larger one keeps the larger allocation, and the refilled contents are
    /// the freshly computed projection, not the previous fit's values.
    #[test]
    fn capacity_is_reused_across_fits_and_the_refill_is_fresh() {
        let mut next = rng(0x5c2a_7c4e_0000_0002);
        let mut cache = SetupCache::new();
        let large = palette(&mut next, 9);
        let large_pixel = pixel(&mut next);
        let _ = fit_multiple_colors_with(&large_pixel, &large, &mut cache);
        let (large_capacity, large_pointer) = FIT_SCRATCH.with(|scratch| {
            let scratch = scratch.borrow();
            (scratch.projection.capacity(), scratch.projection.as_ptr())
        });
        assert!(large_capacity >= 9);

        let small = palette(&mut next, 3);
        let small_pixel = pixel(&mut next);
        let got = fit_multiple_colors_with(&small_pixel, &small, &mut cache).expect("fit");
        let want = fit_multiple_colors_reference(&small_pixel, &small).expect("oracle");
        assert!(same_fit(&got, &want), "the refilled fit must match the oracle");

        FIT_SCRATCH.with(|scratch| {
            let scratch = scratch.borrow();
            assert_eq!(scratch.projection.len(), 3, "the refill must be exactly count long");
            assert_eq!(scratch.constrained.len(), 3, "the refill must be exactly count long");
            assert!(scratch.projection.capacity() >= 9, "the larger allocation must be kept");
            assert_eq!(scratch.projection.as_ptr(), large_pointer, "the refill must not reallocate");
            // The projection is the fit's own dot products, recomputed from
            // zero: a stale value would show up here.
            for column in 0..3 {
                let mut expected = 0f64;
                for channel in 0..3 { expected += small[column * 4 + channel] as f64 * small_pixel[channel]; }
                assert_eq!(scratch.projection[column].to_bits(), expected.to_bits(),
                    "projection entry {column} must be freshly accumulated");
            }
            // The row arena is one allocation, so nothing can expose capacity
            // past the slot a fit is using; the arena is only ever grown.
            assert!(scratch.rows.len() >= 2 * (3 + 1) * 3, "the row arena must stay large enough for count");
        });
    }

    /// The constraint branch (the one that adds a row per active-set iteration)
    /// must keep every live row in its own slot, and the answer must own its
    /// buffers rather than alias a scratch row.
    ///
    /// The rows share one arena, so "no aliasing" is a statement about the
    /// slots: each live row is a distinct, non-overlapping `count`-long window,
    /// and the answer's buffer is outside the arena entirely.
    #[test]
    fn live_rows_never_alias_and_the_answer_owns_its_buffers() {
        let mut next = rng(0x5c2a_7c4e_0000_0003);
        let mut cache = SetupCache::new();
        let mut checked = 0usize;
        for count in 3..=9usize {
            for _palette in 0..4 {
                let colors = palette(&mut next, count);
                for _sample in 0..4 {
                    let sample = pixel(&mut next);
                    ACTIVE_SET.with(|c| c.set((0, 0)));
                    let got = fit_multiple_colors_with(&sample, &colors, &mut cache).expect("fit");
                    let want = fit_multiple_colors_reference(&sample, &colors).expect("oracle");
                    assert!(same_fit(&got, &want), "fit differs for count={count}");
                    let (_, added) = ACTIVE_SET.with(|c| c.get());
                    FIT_SCRATCH.with(|scratch| {
                        let scratch = scratch.borrow();
                        // Every slot the fit could have used, both halves.
                        let slots = 2 * (count + 1);
                        assert!(scratch.rows.len() >= slots * count, "the arena must hold every slot");
                        let base = scratch.rows.as_ptr();
                        for slot in 0..slots {
                            let start = base.wrapping_add(slot * count);
                            for other in slot + 1..slots {
                                let other_start = base.wrapping_add(other * count);
                                // Two `count`-long windows at different offsets
                                // cannot overlap, and the offsets are distinct.
                                assert_ne!(start, other_start, "two slots share a start");
                                assert!(start.wrapping_add(count) <= other_start
                                    || other_start.wrapping_add(count) <= start,
                                    "two live rows overlap");
                            }
                        }
                        assert_ne!(got.weights.as_ptr(), scratch.projection.as_ptr(),
                            "the answer must not alias the retained projection");
                        assert_ne!(got.weights.as_ptr(), base,
                            "the answer must not alias the row arena");
                        assert_ne!(got.weights.as_ptr(), scratch.schur.as_ptr(),
                            "the answer must not alias the Schur arena");
                    });
                    if added > 0 { checked += 1; }
                }
            }
        }
        assert!(checked > 0, "the constraint branch must be exercised by this sweep");
    }

    /// The disjointness the fit relies on is enforced by the type system, and
    /// this test pins it at the place the compiler enforces it: `split_at_mut`
    /// hands out a constraint half and a solved half that provably cannot
    /// overlap, and the two halves are then written through at the same time.
    ///
    /// There is no `unsafe` in this module, so this is the whole aliasing
    /// argument: if a later change reached for a raw pointer or a `Cell` to
    /// "simplify" the split, the borrow checker would stop compiling the writes
    /// below, which is the check this test documents.
    #[test]
    fn the_row_halves_are_disjoint_through_the_borrow_checker() {
        let mut rows = vec![0.0f64; 2 * (9 + 1) * 9];
        let count = 9usize;
        let (constraint_rows, solved_rows) = rows.split_at_mut((count + 1) * count);
        // Address arithmetic on the two live halves: they must not overlap.
        let (low, high) = (constraint_rows.as_ptr(), solved_rows.as_ptr());
        assert!(low.wrapping_add(constraint_rows.len()) <= high, "the halves must not overlap");
        // Both halves are written through simultaneously: this only compiles
        // because the borrow was split, which is the aliasing property itself.
        for slot in 0..=count {
            let constraint = &mut constraint_rows[slot * count..][..count];
            let solved = &mut solved_rows[slot * count..][..count];
            constraint.fill(1.0);
            solved.fill(2.0);
        }
        assert!(constraint_rows.iter().all(|&v| v == 1.0), "the constraint half must hold its own writes");
        assert!(solved_rows.iter().all(|&v| v == 2.0), "the solved half must hold its own writes");
        // And the two halves tile the arena exactly, with no gap and no overlap.
        assert_eq!(constraint_rows.len() + solved_rows.len(), 2 * (count + 1) * count);
    }

    /// A negative stress pass: fits of different sizes interleave on one thread,
    /// so every fit runs on an arena an earlier, differently sized fit filled.
    /// The answer is written into the caller's buffer, which is resized per fit
    /// and never retained by the scratch; this pins that a reused answer buffer
    /// of a different size costs correctness nothing.
    #[test]
    fn reusing_one_answer_buffer_across_sizes_keeps_every_fit_exact() {
        let mut next = rng(0x5c2a_7c4e_0000_0007);
        let mut cache = SetupCache::new();
        let mut checked = 0usize;
        // Interleave palettes of different sizes on one thread, so every fit
        // runs on an arena some earlier, differently sized fit filled.
        for round in 0..24usize {
            let count = 3 + round % 7;
            let colors = palette(&mut next, count);
            for _sample in 0..5 {
                let sample = pixel(&mut next);
                let got = fit_multiple_colors_with(&sample, &colors, &mut cache).expect("fit");
                let want = fit_multiple_colors_reference(&sample, &colors).expect("oracle");
                assert!(same_fit(&got, &want), "fit {round} differs at count={count}");
                // The answer owns exactly what the oracle produced: nothing of
                // the arena leaked into it, and its length is `count`.
                assert_eq!(got.weights.len(), count, "the answer must be exactly count long");
                // The scratch holds no answer buffer at all: the answer lives
                // in the caller's storage, so the arena is the only retained
                // state and it still holds this fit's slots.
                FIT_SCRATCH.with(|scratch| {
                    let scratch = scratch.borrow();
                    assert!(scratch.rows.len() >= 2 * (count + 1) * count,
                        "the row arena must still hold this fit's slots");
                });
                checked += 1;
            }
        }
        assert!(checked >= 100, "the sweep must not thin out: {checked}");
    }

    /// The into-form is the owned form with a different destination: every
    /// field must be bit-identical to the oracle, and a caller that keeps one
    /// `BlendFit` across a run of fits must not reallocate it. The second half
    /// is the point of the change, so it is pinned by address, not by timing.
    #[test]
    fn the_into_form_matches_the_oracle_and_reuses_one_buffer() {
        let mut next = rng(0x5c2a_7c4e_0000_0009);
        let mut cache = SetupCache::new();
        let mut out = BlendFit::default();
        let mut checked = 0usize;
        for round in 0..20usize {
            let count = 3 + round % 7;
            let colors = palette(&mut next, count);
            for _sample in 0..4 {
                let sample = pixel(&mut next);
                fit_multiple_colors_into_with(&sample, &colors, &mut cache, &mut out).expect("fit");
                let want = fit_multiple_colors_reference(&sample, &colors).expect("oracle");
                assert_eq!(out.color.len(), want.color.len(), "the color must be four channels");
                assert_eq!(out.weights.len(), want.weights.len(), "the answer must be exactly count long");
                assert!(out.color.iter().zip(&want.color).all(|(a, b)| a.to_bits() == b.to_bits()),
                    "the into-form's color differs at count={count}");
                assert!(out.weights.iter().zip(&want.weights).all(|(a, b)| a.to_bits() == b.to_bits()),
                    "the into-form's weights differ at count={count}");
                assert_eq!(out.self_weight.to_bits(), want.self_weight.to_bits(),
                    "the into-form's self weight differs at count={count}");
                checked += 1;
            }
        }
        assert!(checked >= 60, "the sweep must not thin out: {checked}");
        // One buffer, one allocation: after the first fit the capacity is at
        // least the largest count the sweep used, and the address never moves.
        let (weights, color) = (out.weights.as_ptr(), out.color.as_ptr());
        let capacity = out.weights.capacity();
        for round in 0..12usize {
            let count = 3 + round % 7;
            let colors = palette(&mut next, count);
            fit_multiple_colors_into_with(&pixel(&mut next), &colors, &mut cache, &mut out).expect("fit");
            assert_eq!(out.weights.as_ptr(), weights, "the answer buffer must not be reallocated");
            assert_eq!(out.color.as_ptr(), color, "the color buffer must not be reallocated");
            assert_eq!(out.weights.capacity(), capacity, "the answer capacity must not change");
        }
    }

    /// The two- and one-color arms of the into-form are the same values the
    /// owned form produced, and they too write into the caller's buffer.
    #[test]
    fn the_into_form_covers_the_short_palette_arms() {
        let mut next = rng(0x5c2a_7c4e_0000_000a);
        let mut out = BlendFit::default();
        for count in [1usize, 2] {
            let colors = palette(&mut next, count);
            for _sample in 0..8 {
                let sample = pixel(&mut next);
                fit_prepared_blend_into(&sample, &colors, &mut out).expect("fit");
                let want = fit_prepared_blend(&sample, &colors).expect("oracle");
                assert_eq!(out.color.len(), want.color.len());
                assert_eq!(out.weights.len(), want.weights.len());
                assert!(out.color.iter().zip(&want.color).all(|(a, b)| a.to_bits() == b.to_bits()),
                    "the short-palette color differs at count={count}");
                assert!(out.weights.iter().zip(&want.weights).all(|(a, b)| a.to_bits() == b.to_bits()),
                    "the short-palette weights differ at count={count}");
                assert_eq!(out.self_weight.to_bits(), want.self_weight.to_bits());
            }
        }
    }

    /// A palette larger than the practical bound is not truncated: the arenas
    /// grow to it and the fit stays bit-exact. The public entry point can be
    /// called with up to nine colors, but nothing in this code assumes it, so a
    /// wider call must be served, not clipped.
    #[test]
    fn a_palette_past_the_practical_bound_grows_the_arena_instead_of_truncating() {
        let mut next = rng(0x5c2a_7c4e_0000_0008);
        let mut cache = SetupCache::new();
        let count = MAX_FIT_COLORS + 4;
        let colors = palette(&mut next, count);
        for _sample in 0..6 {
            let sample = pixel(&mut next);
            let got = fit_multiple_colors_with(&sample, &colors, &mut cache).expect("fit");
            let want = fit_multiple_colors_reference(&sample, &colors).expect("oracle");
            assert!(same_fit(&got, &want), "an oversized palette must be answered exactly");
            assert_eq!(got.weights.len(), count, "no color may be dropped");
        }
        FIT_SCRATCH.with(|scratch| {
            let scratch = scratch.borrow();
            assert!(scratch.rows.len() >= 2 * (count + 1) * count, "the arena must have grown");
        });
        // A fit back at the practical bound still works on the larger arena.
        let small = palette(&mut next, MAX_FIT_COLORS);
        let sample = pixel(&mut next);
        let got = fit_multiple_colors_with(&sample, &small, &mut cache).expect("fit");
        let want = fit_multiple_colors_reference(&sample, &small).expect("oracle");
        assert!(same_fit(&got, &want), "a smaller fit after a larger one must stay exact");
    }

    /// The declined fast path (a non-finite projection) runs the general branch
    /// and must leave the scratch consistent for the next fit.
    #[test]
    fn the_declined_fast_path_leaves_the_scratch_reusable() {
        let mut next = rng(0x5c2a_7c4e_0000_0004);
        let mut cache = SetupCache::new();
        let colors = palette(&mut next, 5);
        for special in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut sample = pixel(&mut next);
            sample[1] = special;
            let got = fit_multiple_colors_with(&sample, &colors, &mut cache).expect("fit");
            let want = fit_multiple_colors_reference(&sample, &colors).expect("oracle");
            assert!(same_fit(&got, &want), "the general branch must match the oracle for {special}");
            FIT_SCRATCH.with(|scratch| {
                let _guard = scratch.try_borrow_mut().expect("the fit left its scratch borrow live");
            });
        }
        // A normal fit after the non-finite ones is still exact.
        let sample = pixel(&mut next);
        let got = fit_multiple_colors_with(&sample, &colors, &mut cache).expect("fit");
        let want = fit_multiple_colors_reference(&sample, &colors).expect("oracle");
        assert!(same_fit(&got, &want), "a fit after the non-finite ones must still be exact");
    }

    /// The scratch is thread-local: another thread gets its own buffers, and
    /// the answers are identical to this thread's.
    #[test]
    fn another_thread_gets_its_own_scratch_and_the_same_answers() {
        let mut next = rng(0x5c2a_7c4e_0000_0005);
        let mut cases = Vec::new();
        for count in 3..=7usize {
            let colors = palette(&mut next, count);
            let sample = pixel(&mut next);
            let mut cache = SetupCache::new();
            let want = fit_multiple_colors_with(&sample, &colors, &mut cache).expect("fit");
            cases.push((colors, sample, want));
        }
        let expected: Vec<BlendFit> = cases.iter().map(|(_, _, want)| want.clone()).collect();
        let inputs: Vec<(Vec<f32>, [f64; 4])> = cases.into_iter().map(|(colors, sample, _)| (colors, sample)).collect();
        let got = std::thread::spawn(move || {
            let mut cache = SetupCache::new();
            inputs.iter().map(|(colors, sample)| fit_multiple_colors_with(sample, colors, &mut cache).expect("fit")).collect::<Vec<_>>()
        }).join().expect("the fit thread must not panic");
        assert_eq!(got.len(), expected.len());
        for (got, want) in got.iter().zip(&expected) { assert!(same_fit(got, want), "a thread's fit differs"); }
    }

    /// The Schur accumulation writes every cell before it reads it, so the
    /// zero-fill that used to precede it was dead. This is the negative
    /// control: fill the arena with a value that would poison any sum that
    /// read it, run the fit, and demand the oracle's exact bits. If a later
    /// change reintroduced a read of the arena's previous contents, the
    /// poisoned entries would surface here as a different answer.
    #[test]
    fn the_schur_arena_is_never_read_before_it_is_written() {
        let mut next = rng(0x5c2a_7c4e_0000_0006);
        let mut cache = SetupCache::new();
        let mut checked = 0usize;
        let mut multi_iteration = 0usize;
        for count in 3..=9usize {
            for _palette in 0..4 {
                let colors = palette(&mut next, count);
                for _sample in 0..4 {
                    let sample = pixel(&mut next);
                    // Poison both arenas with a large finite value and a NaN
                    // tail: any cell the accumulation read instead of writing
                    // would carry it into the answer.
                    FIT_SCRATCH.with(|scratch| {
                        let mut scratch = scratch.borrow_mut();
                        scratch.schur.fill(1.0e300);
                        scratch.right.fill(f64::NAN);
                    });
                    ACTIVE_SET.with(|c| c.set((0, 0)));
                    let got = fit_multiple_colors_with(&sample, &colors, &mut cache).expect("fit");
                    let want = fit_multiple_colors_reference(&sample, &colors).expect("oracle");
                    assert!(same_fit(&got, &want), "a poisoned arena changed the answer at count={count}");
                    let (iterations, _) = ACTIVE_SET.with(|c| c.get());
                    if iterations > 1 { multi_iteration += 1; }
                    checked += 1;
                }
            }
        }
        assert!(checked >= 100, "the sweep must not thin out: {checked}");
        assert!(multi_iteration > 0, "the Schur path must actually run: {multi_iteration}");
    }

    /// The Schur build now writes only the lower triangle and the diagonal, so
    /// the cells with `row < column` keep whatever the arena held. This is the
    /// negative control for that skip: poison exactly those cells with a NaN
    /// before every fit and demand the oracle's exact bits. If the build ever
    /// read one of them — or if a consumer of the buffer ever started reading
    /// the upper triangle — the NaN would reach the answer.
    #[test]
    fn the_skipped_schur_cells_cannot_reach_the_answer() {
        let mut next = rng(0x5c2a_7c4e_0000_0009);
        let mut cache = SetupCache::new();
        let mut checked = 0usize;
        let mut multi_iteration = 0usize;
        for count in 3..=9usize {
            for _palette in 0..4 {
                let colors = palette(&mut next, count);
                for _sample in 0..4 {
                    let sample = pixel(&mut next);
                    // Every cell the build skips is a NaN; every cell it writes
                    // is overwritten before the factorization reads it.
                    FIT_SCRATCH.with(|scratch| {
                        let mut scratch = scratch.borrow_mut();
                        scratch.schur.fill(f64::NAN);
                        scratch.right.fill(f64::NAN);
                    });
                    ACTIVE_SET.with(|c| c.set((0, 0)));
                    let got = fit_multiple_colors_with(&sample, &colors, &mut cache).expect("fit");
                    let want = fit_multiple_colors_reference(&sample, &colors).expect("oracle");
                    assert!(same_fit(&got, &want), "a skipped Schur cell changed the answer at count={count}");
                    let (iterations, _) = ACTIVE_SET.with(|c| c.get());
                    if iterations > 1 { multi_iteration += 1; }
                    checked += 1;
                }
            }
        }
        assert!(checked >= 100, "the sweep must not thin out: {checked}");
        assert!(multi_iteration > 0, "the Schur path must actually run: {multi_iteration}");
    }
}

#[cfg(test)]
mod stack_selection_equivalence {
    //! `collect_blend_indices` writes into the caller's nine-entry buffer. The
    //! retained `Vec` version is the oracle: the returned prefix must be the
    //! same colors in the same order, the length must match, and nothing past
    //! the returned prefix may be written.
    use super::*;

    fn check(indices: &[i32], width: usize, height: usize, pixel: usize, directions: &[(i64, i64)],
        source: i32, target: i32) -> usize {
        let expected = collect_blend_indices_reference(indices, width, height, pixel, directions, source, target);
        let mut storage = [-777; 9];
        let len = collect_blend_indices_into(indices, width, height, pixel, directions, source, target, &mut storage);
        assert_eq!(&storage[..len], expected.as_slice(),
            "width={width} height={height} pixel={pixel} source={source} target={target} directions={directions:?}");
        assert_eq!(len, expected.len());
        assert!(storage[len..].iter().all(|&value| value == -777), "write beyond returned prefix");
        len
    }

    #[test]
    fn corners_mixed_dimensions_direction_counts_and_remaps_are_exact() {
        let mut directions = Vec::new();
        for y in -3..=3 { for x in -3..=3 { directions.push((x, y)); } }
        assert!(directions.len() > 24);
        let lists: Vec<&[(i64, i64)]> = vec![&[], &directions[..1], &EIGHT_DIRECTIONS, &TWENTY_FOUR_DIRECTIONS, &directions];
        let mut checked = 0;
        for &(width, height) in &[(1, 1), (2, 1), (1, 2), (2, 2), (2, 7), (7, 2), (7, 9)] {
            let indices: Vec<i32> = (0..width * height).map(|p| ((p * 7 + p / 3) % 11) as i32 - 1).collect();
            for pixel in 0..width * height {
                for &dirs in &lists {
                    for &(source, target) in &[(-1, -1), (0, 0), (0, 2), (2, 0), (indices[pixel], indices[pixel]), (indices[pixel], 9)] {
                        assert!(check(&indices, width, height, pixel, dirs, source, target) <= 9);
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 3_000);
    }

    #[test]
    fn ninth_unique_stops_before_tenth_and_remap_deduplicates_raw_neighbors() {
        let indices: Vec<i32> = (0..16).collect();
        let first_ring = [1, 4, 5, 2, 3, 6, 9, 8, 7, 10, 11, 12];
        let dirs: Vec<(i64, i64)> = first_ring.iter().map(|&p| ((p % 4) as i64, (p / 4) as i64)).collect();
        assert_eq!(check(&indices, 4, 4, 0, &dirs, -1, -1), 9);
        assert_eq!(collect_blend_indices_reference(&indices, 4, 4, 0, &dirs, -1, -1), [0, 1, 4, 5, 2, 3, 6, 9, 8]);
        // Every raw neighbor maps to the center. Deduplicating before remapping
        // would incorrectly return multiple copies of seven instead of one.
        assert_eq!(check(&[5; 16], 4, 4, 0, &dirs, 5, 7), 1);
        assert_eq!(collect_blend_indices_reference(&[5; 16], 4, 4, 0, &dirs, 5, 7), [7]);
        assert_eq!(check(&indices, 4, 4, 0, &dirs, 0, 4), 9);
        assert_eq!(collect_blend_indices_reference(&indices, 4, 4, 0, &dirs, 0, 4), [4, 1, 5, 2, 3, 6, 9, 8, 7]);
        assert_eq!(check(&indices, 4, 4, 0, &dirs, -1, -1), 9);
        assert_eq!(check(&indices, 4, 4, 0, &[], 0, -3), 1);
        assert_eq!(collect_blend_indices_reference(&indices, 4, 4, 0, &[], 0, -3), [-3]);
        assert_eq!(check(&indices, 4, 4, 0, &dirs[..1], 1, -3), 2);
        assert_eq!(collect_blend_indices_reference(&indices, 4, 4, 0, &dirs[..1], 1, -3), [0, -3]);
    }

    #[test]
    fn repeated_unique_order_and_stale_storage_do_not_leak() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        for width in 1..=11usize {
            let height = 13;
            let indices: Vec<i32> = (0..width * height).map(|_| {
                state ^= state << 13; state ^= state >> 7; state ^= state << 17;
                (state % 13) as i32
            }).collect();
            let dirs: Vec<(i64, i64)> = (0..30).map(|n| ((n % 7) as i64 - 3, (n / 7) as i64 - 2)).collect();
            for pixel in 0..indices.len() {
                let center = indices[pixel];
                check(&indices, width, height, pixel, &dirs, center, 99);
                check(&indices, width, height, pixel, &dirs, 0, -1);
            }
        }
    }

    #[test]
    fn collection_at_nine_ignores_later_remap_and_oob_guards() {
        let indices: Vec<i32> = (0..16).collect();
        let dirs: Vec<(i64, i64)> = vec![(1, 0), (0, 1), (1, 1), (2, 0), (0, 2), (2, 1), (1, 2), (2, 2), (3, 0),
            (99, 99), (2, -100), (-99, 0), (-1, -1), (0, 0)];
        assert_eq!(check(&indices, 4, 4, 0, &dirs, 12, 13), 9);
        assert_eq!(check(&indices, 4, 4, 0, &dirs, 0, 13), 9);
    }
}
