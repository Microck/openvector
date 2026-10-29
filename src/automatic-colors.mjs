// The compiled palette-count classifier splits 62 numeric features. Each
// tuple is [feature, threshold, less-than child, other child]; leaves are the
// original labels. Equal and unordered values take the other child, matching
// the original x87 FCOMIP/JBE branches rather than mathematical >= tests.
const paletteCountModel = [50, 2.69034,
  [50, 1.73555,
    [51, 217.677, [37, 35.25, '3', '2'], '2'],
    [51, 995.704,
      [36, 2.5, '2',
        [39, 0.351533, '3',
          [13, 0.938407, [50, 2.52056, '13', '4'], '13']]],
      '2']],
  [40, 0.288933,
    [39, 0.333629, '3',
      [15, 6.5,
        [12, 2.2514,
          [36, 3.5, '3', [39, 0.413465, '3', '4']],
          '13'],
        '4']],
    [13, 2.04511,
      [15, 7.5,
        [51, 2004.84,
          [36, 6.5, '13', [10, 1.24186, '8', '13']],
          [0, 986, '2', '3']],
        [36, 5.5,
          [41, 0.136787, '5', '4'],
          [3, 8.5,
            [16, 6.5, '13', [42, 0.313304, '6', '7']],
            [44, 0.129347, '8', '9']]]],
      [36, 7.5,
        [50, 3.56775,
          [51, 3296.27, '13', '2'],
          [41, 0.160751, '5', '13']],
        '13']]]];

/** Decode a model label from the original registered 2..13 domain. Mode 2
 * means the controller chooses its minimum-score candidate; its registered
 * count of 12 is a bound, not an unconditional recommendation of 12 colors. */
export function decodePaletteCountLabel(label) {
  return { label, mode:label === '2' ? 0 : label === '13' ? 2 : 1,
    colorCount:label === '13' ? 12 : Number(label) };
}

/** Internal model kernel. Its producer owns the 62-double feature contract;
 * derived NaNs are observable native states and must not be rejected here. */
export function classifyPaletteFeatures(features) {
  let node = paletteCountModel;
  while (typeof node !== 'string') {
    node = features[node[0]] < node[1] ? node[2] : node[3];
  }
  return decodePaletteCountLabel(node);
}

// The native x86 code turns an invalid operation on non-NaN operands (0/0,
// inf-inf) into the default "indefinite" NaN, 0xFFF8000000000000. AArch64
// produces 0x7FF8000000000000 instead, and these features are compared
// bit-exactly, so generated NaNs take the x86 pattern on every host.
// NaN operands propagate unchanged, as they do on both architectures.
const X86_INDEFINITE = new Float64Array(new BigUint64Array([0xfff8000000000000n]).buffer)[0];
const x86Double = (result, ...operands) =>
  Number.isNaN(result) && !operands.some(Number.isNaN) ? X86_INDEFINITE : result;

/** Build the original 62 features from the retained 2..12 candidates. This
 * internal producer receives validated BGRA and original-order candidates;
 * scores precede palette quantization, and membership weights follow the last
 * assignment pass. Empty peak arrays reproduce a native fatal as a diagnostic. */
export function buildPaletteFeatures(bgra, candidates, peakWeights) {
  if (peakWeights.length === 0) {
    throw new Error('Automatic color selection has no histogram peaks. The original classifier faults while reading the empty peak-weight array.');
  }
  const totalOrder = [{ value:1e6, id:-1 }, { value:1e6, id:-1 }];
  const errorOrder = [{ value:1e6, id:-1 }, { value:1e6, id:-1 }];
  let maximumError = candidates[0].score.error;
  let minimumError = maximumError;
  for (let index = 0; index < candidates.length; index++) {
    const { total, error } = candidates[index].score;
    totalOrder.push({ value:total, id:index + 2 });
    errorOrder.push({ value:error, id:index + 2 });
    if (error > maximumError) maximumError = error;
    if (error < minimumError) minimumError = error;
  }
  // Original 0x48cf40 breaks score ties by candidate ID. The only equal
  // complete pairs are the two indistinguishable sentinels, so CRT qsort's
  // unstable partition schedule cannot affect these feature values.
  const compare = (left, right) => left.value > right.value ? 1 :
    left.value < right.value ? -1 : left.id - right.id;
  totalOrder.sort(compare);
  errorOrder.sort(compare);
  const features = new Float64Array(62);
  const distinct = new Set();
  for (let pixel = 0; pixel < bgra.length; pixel += 4) {
    distinct.add(bgra[pixel] | bgra[pixel + 1] << 8 | bgra[pixel + 2] << 16 | bgra[pixel + 3] << 24);
  }
  features[0] = distinct.size;
  features[1] = bgra.length / 4;
  for (let rank = 0; rank < totalOrder.length; rank++) {
    if (rank < 4) {
      features[2 + rank] = totalOrder[rank].id;
      features[6 + rank] = errorOrder[rank].id;
      features[10 + rank] = errorOrder[rank].value;
    }
    if (totalOrder[rank].id > 1) features[14 + totalOrder[rank].id - 2] = rank;
    if (errorOrder[rank].id > 1) features[25 + errorOrder[rank].id - 2] = rank;
  }
  features[36] = peakWeights.length;
  features[37] = Math.min(...peakWeights);
  for (let index = 0; index < candidates.length; index++) {
    const numerator = x86Double(candidates[index].score.error - minimumError, candidates[index].score.error, minimumError);
    const range = x86Double(maximumError - minimumError, maximumError, minimumError);
    features[38 + index] = x86Double(numerator / range, numerator, range);
    features[51 + index] = Math.min(...candidates[index].weights);
  }
  features[49] = minimumError;
  features[50] = maximumError;
  return features;
}
