import { streamGeneralDecimal as streamFloat } from './decimal-format.mjs';

function streamColor(bgra) {
  return `[${Array.from(bgra).reverse().map(streamFloat).join(', ')}]`;
}

/** Internal key kernel over validated settings and float32 BGRA channels. */
export function documentPaletteKey({ category,sensitivity,advanced,antiAlias,backgroundMode,background }) {
  const aliased = !(advanced ? antiAlias : category === 1);
  return `${aliased ? 'aliased' : 'antialiased'}${backgroundMode}${streamColor(background)}${sensitivity}`;
}

/** Internal key kernel over validated document controls and float32 colors. */
export function documentSegmentationKey(settings) {
  const {category,quality,advanced,complexity,minimumPixels,artifactRejection,clusterColors,
    backgroundMode,background,paletteMode,palette} = settings;
  const controls = advanced ? `adv${complexity} ${minimumPixels} ${artifactRejection} ${Number(clusterColors)}`
    : `basic ${category} ${quality}`;
  let colors = '';
  if (paletteMode === 0 || paletteMode === 1) for (let offset = 0; offset < palette.length; offset += 4)
    colors += streamColor(palette.slice(offset,offset+4));
  return `${backgroundMode} ${streamColor(background)} ${controls} ${['two colors ','fixed ','flexible',''][paletteMode]}${colors}`;
}

export function documentSmoothingKey(settings,editKey = '') {
  return documentSegmentationKey(settings)+editKey+(settings.advanced
    ? ` ${settings.smoothness} ${Number(settings.detectCorners)} ${Number(settings.smoothingAntiAlias)}` : '');
}

export function documentFittingKey(settings,editKey = '') {
  return documentSmoothingKey(settings,editKey)+(settings.advanced ? String(settings.curveComplexity) : '');
}

export function readDocumentSettings(input) {
  if (input === null || typeof input !== 'object' || Array.isArray(input)) throw new Error('Expected document controls.');
  const {category,quality,sensitivity,advanced,antiAlias,backgroundMode,background,complexity,
    minimumPixels,artifactRejection,clusterColors,paletteMode,palette,smoothness,detectCorners,curveComplexity,smoothingAntiAlias} = input;
  for (const [name,value] of [['category',category],['quality',quality],['sensitivity',sensitivity],
    ['artifactRejection',artifactRejection],['paletteMode',paletteMode]]) {
    const minimum = name === 'category' ? -1 : 0;
    // Selected batch controls can retain auto sentinels across profile changes.
    // Basic processing kernels still require a classified category and quality.
    const maximum = ['category','quality','paletteMode'].includes(name) ? 3 : 2;
    if (!Number.isInteger(value) || value < minimum || value > maximum) throw new Error(`Document ${name} must be an integer from ${minimum} through ${maximum}.`);
  }
  for (const [name,value] of [['advanced',advanced],['antiAlias',antiAlias],['clusterColors',clusterColors],
    ['detectCorners',detectCorners],['smoothingAntiAlias',smoothingAntiAlias]])
    if (typeof value !== 'boolean') throw new Error(`Document ${name} must be boolean.`);
  for (const [name,value] of [['smoothness',smoothness],['curveComplexity',curveComplexity]])
    if (!Number.isInteger(value) || value < 1 || value > 12) throw new Error(`Document ${name} must be an integer from 1 through 12.`);
  if (!Number.isInteger(complexity) || complexity < 1 || complexity > 12) throw new Error('Document complexity must be an integer from 1 through 12.');
  if (!Number.isInteger(minimumPixels) || minimumPixels < 0 || minimumPixels > 2147483647) throw new Error('Document minimumPixels must be a nonnegative int32 integer.');
  if (!Number.isInteger(backgroundMode) || backgroundMode < -2147483648 || backgroundMode > 2147483647) {
    throw new Error('Document backgroundMode must be a signed int32 integer.');
  }
  if ((!Array.isArray(background) && !(background instanceof Float32Array)) || background.length !== 4) {
    throw new Error('Document background must contain four finite unit BGRA channels.');
  }
  for (const value of background) {
    if (!Number.isFinite(value) || value < 0 || value > 1) throw new Error('Document background must contain four finite unit BGRA channels.');
  }
  if ((!Array.isArray(palette) && !(palette instanceof Float32Array)) || palette.length % 4 !== 0 || palette.length > 1024)
    throw new Error('Document palette must contain up to 256 BGRA colors.');
  for (const value of palette)
    if (!Number.isFinite(value) || value < 0 || value > 1) throw new Error('Document palette must contain finite unit BGRA channels.');
  return {category,quality,sensitivity,advanced,antiAlias,backgroundMode,background:Float32Array.from(background),
    complexity,minimumPixels,artifactRejection,clusterColors,paletteMode,palette:Float32Array.from(palette),
    smoothness,detectCorners,curveComplexity,smoothingAntiAlias};
}

/** Fresh scalar state shared by document construction and batch profiles. */
export function defaultDocumentSettings() {
  return {category:-1,quality:1,sensitivity:1,advanced:false,antiAlias:true,
    backgroundMode:-1,background:new Float32Array(4),complexity:5,minimumPixels:0,artifactRejection:0,
    clusterColors:true,paletteMode:1,palette:new Float32Array(),smoothness:6,detectCorners:true,
    curveComplexity:6,smoothingAntiAlias:true};
}

export const advancedBatchControls = ['sensitivity','antiAlias','complexity','minimumPixels','artifactRejection',
  'clusterColors','smoothness','detectCorners','curveComplexity','smoothingAntiAlias'];

/** Basic batches resolve auto sentinels. Advanced batches retain them and
 * use the existing document profile; their fixed palette evaluates one count. */
export function readBatchControls(input) {
  if (input === null || typeof input !== 'object' || Array.isArray(input))
    throw new Error('Batch image processing requires controls with an extension.');
  const {advanced=false,category=3,quality=3,paletteMode=3,colorCount=advanced ? 2 : 0,backgroundMode=1} = input;
  if (typeof advanced !== 'boolean') throw new Error('Batch advanced must be boolean.');
  for (const [name,value] of [['category',category],['quality',quality],['paletteMode',paletteMode]])
    if (!Number.isInteger(value) || value<0 || value>3)
      throw new Error(`Batch ${name} must be an integer from 0 through 3.`);
  if (!advanced && paletteMode === 0) throw new Error('Batch two-color processing requires paletteMode 1 and colorCount 2.');
  if (!Number.isInteger(colorCount) || (advanced ? colorCount<2 || colorCount>24 : colorCount!==0 && (colorCount<2 || colorCount>12)))
    throw new Error(advanced ? 'Advanced batch colorCount must be an integer from 2 through 24.' :
      'Batch colorCount must be 0 for automatic or an integer from 2 through 12.');
  if (![0,1,3].includes(backgroundMode)) throw new Error('Batch backgroundMode must be 0, 1 or 3.');
  const controls = {advanced,category,quality,paletteMode,colorCount,backgroundMode,palette:[]};
  if (advanced || Object.hasOwn(input,'palette') || advancedBatchControls.some(name => Object.hasOwn(input,name))) {
    const profile = readDocumentSettings({...defaultDocumentSettings(),...input,advanced});
    controls.palette = Array.from(profile.palette);
    for (const name of advancedBatchControls)
      if (advanced || Object.hasOwn(input,name)) controls[name] = profile[name];
  }
  return controls;
}

/** Commit the validated batch selection before loading its next file. Auto
 * sentinels stay unresolved until that file's processing tasks run. */
export function batchDocumentSettings(current,controls) {
  return readDocumentSettings({...current,...controls,
    background:Float32Array.of(1,1,1,1)});
}

