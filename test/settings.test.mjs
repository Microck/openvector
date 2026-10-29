import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { resolveParameters } from '../src/settings.mjs';

const reference = JSON.parse(await readFile(new URL('./fixtures/settings-reference.json', import.meta.url), 'utf8'));

test('advanced controls match all 648 original host-CPU arithmetic cases exactly', () => {
  for (const sample of reference.cases) {
    if (sample.stage === 'segmentation') {
      const parameters = resolveParameters({
        mode: 'advanced', stage: 'segmentation', complexity: sample.complexity,
        minimumPixels: sample.minimumPixels,
        artifactRejection: ['none', 'moderate', 'aggressive'][sample.rejection],
        clusterColors: sample.cluster,
      });
      assert.deepEqual({
        lambda: ['lambda_pre', 'lambda_initial', 'lambda_final'].map(name => parameters[`Segmenter::${name}`]),
        minimumPixels: parameters['Segmenter::min_num_pixels'],
        antiAlias: parameters['Shared::is_anti_aliased'],
        shadowing: Number(parameters.shadowing),
        clusterColors: Number(parameters.clusterColors),
      }, sample.expected);
    } else {
      for (const stage of ['smoothing', 'fitting']) {
        const parameters = resolveParameters({
          mode: 'advanced', stage, smoothness: sample.smoothness,
          curveComplexity: sample.complexity, antiAlias: sample.antiAlias, detectCorners: sample.corners,
        });
        assert.deepEqual({
          measurementTypes: parameters['ContourSmoother::measurement_types'],
          priorTypes: parameters['ContourSmoother::prior_types'],
          priorStrengths: parameters['ContourSmoother::prior_strengths'],
          lengthPenalties: parameters['ContourSmoother::length_penalty_weights'],
          punctureCorners: parameters['ContourSmoother::do_puncture_corners'],
          enabled: parameters['ContourSmoother::is_enabled'],
          bezierThresholds: [parameters['BezierFitter::stat_thresh_initial'], parameters['BezierFitter::stat_thresh_final']],
          antiAlias: parameters['Shared::is_anti_aliased'],
        }, sample.expected);
      }
    }
  }
});

test('basic profiles map category and quality to the original image-type codes', () => {
  const categories = ['photograph', 'artwork-with-blending', 'artwork-without-blending'];
  const expectedCodes = [[8, 7, 6], [5, 4, 3], [2, 1, 0]];
  for (const [categoryIndex, category] of categories.entries()) {
    for (const [qualityIndex, quality] of ['high', 'medium', 'low'].entries()) {
      const parameters = resolveParameters({ mode: 'basic', stage: 'segmentation', category, quality });
      assert.equal(Object.keys(parameters).length, 100);
      assert.equal(parameters['Shared::image_type_code'], expectedCodes[categoryIndex][qualityIndex]);
      assert.equal(parameters['SubPixelSegmenter::color_cluster_margin'], Math.fround(0.1));
    }
  }
});

test('each document owns its mutable parameter arrays', () => {
  const options = { mode: 'basic', stage: 'smoothing', category: 'photograph', quality: 'high' };
  const first = resolveParameters(options);
  const second = resolveParameters(options);
  first['ContourSmoother::prior_strengths'][0] = -1;
  assert.notEqual(second['ContourSmoother::prior_strengths'][0], -1);
  assert.deepEqual(resolveParameters(options), second);
});

test('invalid or missing controls fail at the API boundary', () => {
  const options = { mode: 'advanced', stage: 'segmentation', complexity: 6, minimumPixels: 0, artifactRejection: 'moderate', clusterColors: true };
  for (const complexity of [0, 13, 1.5, NaN, '6', undefined]) {
    assert.throws(() => resolveParameters({ ...options, complexity }), /complexity must be an integer from 1 to 12/);
  }
  assert.throws(() => resolveParameters({ ...options, artifactRejection: 'unknown' }), /artifactRejection/);
  assert.throws(() => resolveParameters({ ...options, clusterColors: undefined }), /clusterColors/);
  assert.throws(() => resolveParameters(null), /Settings must be an object/);
});
