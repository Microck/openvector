import { segmentWithMoments } from './segmentation.mjs';
import { readRasterInput } from './region-data.mjs';
import { prepareImagePixels } from './image-preparation.mjs';
import { reduceWorkingImage } from './image-resize.mjs';
import { blurClassifierPixels, classifyImageFeatures, measureClassifierGradients,
  measureClassifierEdges, measureClassifierRegions, measureClassifierColors } from './image-classification.mjs';

/** Complete fresh decoded-image analysis on the caller's application session.
 * Native classification changes Shared fields and rebuilds working pixels from
 * the filtered source cache. Keep raw palette scores for feature inspection;
 * the later recommendation writes have their own owned output. */
export function analyzeImage(input, session) {
  if (input === null || typeof input !== 'object') throw new Error('Expected an image analysis object.');
  const { parameters, paletteControls, extension } = input;
  if (paletteControls === null || typeof paletteControls !== 'object') {
    throw new Error('Image analysis requires explicit paletteControls.');
  }
  if (typeof parameters?.shadowing !== 'boolean' || typeof parameters?.clusterColors !== 'boolean') {
    throw new Error('Image analysis requires explicit shadowing and clusterColors switches.');
  }
  const initial = prepareImageState(input);
  return analyzePreparedImage({image:initial,parameters,paletteControls,extension},session);
}

/** Internal shared controller over an already prepared, owned source. Document
 * callers must not repeat decoded alpha handling on this preprocessed cache. */
export function analyzePreparedImage({image:initial,parameters,paletteControls,extension},session) {
  const imageAnalysis = classifyImageState({ image:initial, extension, parameters });
  const { imageType, quality } = imageAnalysis.classification;
  const resolved = { ...structuredClone(parameters), 'Shared::image_type_code':(2 - imageType) * 3 + 2 - quality,
    'Shared::is_anti_aliased':Number(imageType !== 2) };
  const working = reduceWorkingImage({ ...initial.source, matte:1,
    antiAlias:resolved['Shared::is_anti_aliased'] });
  const { bins, connected, costPerColor } = paletteControls;
  const paletteAnalysis = session.analyzeColorCounts({ ...working, bins, connected, costPerColor, antiAlias:imageType !== 2 });
  const recommendationScores = Float32Array.from(paletteAnalysis.candidates, candidate =>
    paletteAnalysis.classification.mode === 2 ? candidate.score.total
      : Number(candidate.colorCount !== paletteAnalysis.classification.colorCount));
  return { image:{ source:initial.source, working }, parameters:resolved, imageAnalysis, paletteAnalysis,
    recommendationScores, randomState:session.randomState() };
}

/** Internal fresh image state over decoded bytes and explicit native controls.
 * Source is the full preprocessed cache; working is its independent reduction.
 * Parameter changes and native GUI profile/cache invalidation are separate. */
export function prepareImageState(input) {
  const raster = readRasterInput(input);
  const { mode, amount, parameters, hasAlpha } = input;
  if (!Number.isInteger(mode) || mode < 0 || mode > 3 || !Number.isInteger(amount) || amount < 0 || amount > 3) {
    throw new Error('Image preparation requires explicit mode and amount from 0 through 3.');
  }
  const antiAlias = parameters?.['Shared::is_anti_aliased'];
  if (antiAlias !== 0 && antiAlias !== 1) {
    throw new Error('Image preparation requires an explicit anti-alias control of 0 or 1.');
  }
  if (typeof hasAlpha !== 'boolean') throw new Error('Decoded image preparation requires an explicit hasAlpha boolean.');
  const source = prepareDecodedSource(raster,hasAlpha,mode,amount);
  // The native setter exports BGRA, applies initial alpha handling and imports
  // it again. This rebuild enables matte even when the decoded file is opaque.
  const working = reduceWorkingImage({ ...source, antiAlias, matte:1 });
  return { source, working };
}

/** Private canonical source backup for documents before filters/composition. */
export function prepareOriginalImage(input) {
  const raster = readRasterInput(input);
  if (typeof input.hasAlpha !== 'boolean') throw new Error('Decoded image preparation requires an explicit hasAlpha boolean.');
  return prepareDecodedSource(raster,input.hasAlpha,0,0);
}

function prepareDecodedSource(raster,hasAlpha,mode,amount) {
  if (!hasAlpha) {
    // The original GetImageType promotes opaque, nongray palettes to type 7
    // before pixel export. Other types retain their decoded opacity bytes.
    let gray = true;
    const colors = new Set();
    for (let offset = 0; offset < raster.bgra.length; offset += 4) {
      const blue = raster.bgra[offset], green = raster.bgra[offset + 1], red = raster.bgra[offset + 2];
      gray &&= blue === green && green === red;
      colors.add(blue | green << 8 | red << 16);
      if (colors.size > 256) break;
    }
    if (!gray && colors.size <= 256) {
      const bgra = Uint8Array.from(raster.bgra);
      for (let offset = 3; offset < bgra.length; offset += 4) bgra[offset] = 255;
      raster = { ...raster,bgra };
    }
  }
  return prepareImagePixels({ ...raster, mode, amount });
}

/** Internal canonical type/quality analysis of explicit source/working state.
 * Prepared component callers supply their raster in both roles. Full image
 * callers retain the preprocessed source cache across working reduction. No
 * captured native intermediates or guessed profile defaults are inputs. */
export function classifyImageState(input) {
  if (input === null || typeof input !== 'object' || input.parameters === null || typeof input.parameters !== 'object') {
    throw new Error('Expected source/working image state and explicit classifier parameters.');
  }
  const { image, extension, parameters } = input;
  if (image === null || typeof image !== 'object' || image.working === null || typeof image.working !== 'object') {
    throw new Error('Expected source/working image state.');
  }
  const source = readRasterInput(image.source), raster = image.working;
  const { width, height, bgra } = raster;
  if (typeof extension !== 'string') throw new Error('Image classification requires an explicit extension string.');
  if (width < 2 || height < 2) throw new Error('Image classification requires a prepared image of at least 2 by 2.');
  if (width > source.width || height > source.height) throw new Error('Working image must fit within the preprocessed source.');
  const edgeScale = parameters['EdgeImage::edge_weight_scale'];
  const edgeFloor = parameters['EdgeImage::edge_weight_floor'];
  if (!Number.isFinite(edgeScale) || edgeScale < 0 || !Number.isFinite(edgeFloor) || edgeFloor < 0 || edgeFloor > 1) {
    throw new Error('Classifier edge controls require a nonnegative scale and a floor from 0 to 1.');
  }
  // Segmentation validates the raster and consumes the native dispatch controls
  // once. Reuse its owned ordinary region moments for feature slots 5..13.
  const regions = segmentWithMoments({ width, height, bgra, parameters }, source);
  const blurred = { ...source, bgra:blurClassifierPixels(source) };
  const { gradient, blurGradient, rootMeanSquare } = measureClassifierGradients(raster, blurred);
  const features = new Float32Array(20);
  features.set(rootMeanSquare, 0);
  features.set(measureClassifierEdges(raster), 2);
  features.set(measureClassifierRegions(raster, regions), 5);
  const { measurement, features:colorFeatures } = measureClassifierColors(raster, { edgeScale, edgeFloor });
  features.set(colorFeatures, 14);
  return { classification:classifyImageFeatures(features, extension), features, regions,
    blurred, gradient, blurGradient, measurement };
}
