import numericData from './parameter-data.json' with { type: 'json' };
export { CoreSession } from './palette.mjs';
export { decodeImage, resolveImageFilename } from './image-input.mjs';

const stages = ['palette', 'segmentation', 'smoothing', 'fitting', 'classification'];
const categories = ['photograph', 'artwork-with-blending', 'artwork-without-blending', 'unclassified'];
const levels = ['high', 'medium', 'low'];
const rejectionLevels = ['none', 'moderate', 'aggressive'];

function choice(options, name, values) {
  const index = values.indexOf(options[name]);
  if (index === -1) throw new TypeError(`${name} must be one of: ${values.join(', ')}.`);
  return index;
}

function integer(options, name, minimum, maximum) {
  const value = options[name];
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new TypeError(`${name} must be an integer from ${minimum} to ${maximum}.`);
  }
  return value;
}

function boolean(options, name) {
  if (typeof options[name] !== 'boolean') throw new TypeError(`${name} must be true or false.`);
  return options[name];
}

function copyProfile(index) {
  return Object.fromEntries(Object.entries(numericData.profiles[index]).map(([name, value]) =>
    [name, Array.isArray(value) ? [...value] : value]));
}

/**
 * Resolve the numeric parameters for one processing stage. Explicit options
 * avoid inventing image-classifier results or GUI defaults before recovery.
 * Each call owns its arrays so document edits cannot affect another document.
 */
export function resolveParameters(options) {
  if (options === null || typeof options !== 'object' || Array.isArray(options)) {
    throw new TypeError('Settings must be an object.');
  }
  choice(options, 'stage', stages);
  if (options.stage === 'classification') return copyProfile(0);
  choice(options, 'mode', ['basic', 'advanced']);
  if (options.mode === 'basic') {
    const category = choice(options, 'category', categories);
    const quality = choice(options, 'quality', levels);
    if (category === 3 && options.stage !== 'palette') throw new TypeError('Basic processing requires a classified image category.');
    const parameters = copyProfile(options.stage === 'palette' ? 0 : 9 - 3 * category - quality);
    if (options.stage === 'palette') parameters['Shared::is_anti_aliased'] = Number(category === 1);
    return parameters;
  }
  if (options.stage === 'palette') {
    const sensitivity = choice(options, 'colorSensitivity', levels);
    const antiAlias = boolean(options, 'antiAlias');
    const parameters = copyProfile(0);
    parameters['Shared::is_anti_aliased'] = Number(antiAlias);
    parameters['PaletteFinder::use_16_hist_bins'] = Number(sensitivity !== 2);
    parameters['PaletteFinder::use_26_connected_peaks'] = Number(sensitivity !== 0);
    return parameters;
  }
  if (options.stage === 'segmentation') {
    const complexity = integer(options, 'complexity', 1, 12);
    const minimumPixels = integer(options, 'minimumPixels', 0, 2147483647);
    const rejection = choice(options, 'artifactRejection', rejectionLevels);
    const clusterColors = boolean(options, 'clusterColors');
    const parameters = copyProfile(5);
    const lambda = numericData.segmentation[complexity - 1];
    Object.assign(parameters, {
      'Segmenter::lambda_pre': lambda[0],
      'Segmenter::lambda_initial': lambda[1],
      'Segmenter::lambda_final': lambda[2],
      'Segmenter::min_num_pixels': minimumPixels,
      'Shared::is_anti_aliased': Number(rejection !== 2),
    });
    // Native +0x214 controls shadow dispatch, distinct from the registered
    // diagonal-extraction parameter at +0x930. These two unregistered switches
    // are written only by advanced segmentation; other stages preserve them.
    parameters['shadowing'] = rejection === 0;
    parameters['clusterColors'] = clusterColors;
    return parameters;
  }
  const smoothness = integer(options, 'smoothness', 1, 12);
  const complexity = integer(options, 'curveComplexity', 1, 12);
  const antiAlias = boolean(options, 'antiAlias');
  const detectCorners = boolean(options, 'detectCorners');
  const parameters = copyProfile(5);
  const smoothing = numericData.smoothing[Number(antiAlias)][smoothness - 1];
  const thresholds = numericData.curves[complexity - 1];
  // The UI's 12 discrete positions use exact host-x87 results. Replacing these
  // tables with Math.pow changes low bits and can move later fit decisions.
  Object.assign(parameters, {
    'ContourSmoother::measurement_types': Array(3).fill(Number(antiAlias)),
    'ContourSmoother::prior_types': [3, 3, 3],
    'ContourSmoother::prior_strengths': [...smoothing.priorStrengths],
    'ContourSmoother::length_penalty_weights': [...smoothing.lengthPenalties],
    'ContourSmoother::do_puncture_corners': [0, Number(detectCorners), 0],
    'ContourSmoother::is_enabled': [1, Number(detectCorners), 1],
    'BezierFitter::stat_thresh_initial': thresholds[0],
    'BezierFitter::stat_thresh_final': thresholds[1],
    'Shared::is_anti_aliased': Number(antiAlias),
  });
  return parameters;
}
