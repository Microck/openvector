import assert from 'node:assert/strict';
import suppliedPaletteReference from './fixtures/supplied-palette-segmentation-reference.json' with { type: 'json' };
import contourReference from './fixtures/contour-extraction-reference.json' with { type: 'json' };
import antiAliasReference from './fixtures/anti-alias-segmentation-reference.json' with { type: 'json' };
import clusteredReference from './fixtures/clustered-anti-alias-segmentation-reference.json' with { type: 'json' };
import { resolveParameters } from '../src/settings.mjs';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import smoothingReference from './fixtures/smoothing-cli-reference.json' with { type:'json' };
import fittingReference from './fixtures/fitting-cli-reference.json' with { type:'json' };
import svgReference from './fixtures/svg-cli-reference.json' with { type:'json' };
import epsReference from './fixtures/eps-cli-reference.json' with { type:'json' };
import pdfReference from './fixtures/pdf-cli-reference.json' with { type:'json' };
import aiReference from './fixtures/ai-cli-reference.json' with { type:'json' };
import dxfReference from './fixtures/dxf-cli-reference.json' with { type:'json' };
import emfReference from './fixtures/emf-cli-reference.json' with { type:'json' };
import dxfOverlong from './fixtures/dxf-overlong-reference.json' with { type:'json' };
import { createReadStream, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

test('CLI writes all 24 original EMF binary files from stdin and paths with spaces', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-emf-'));
  try {
    assert.equal(emfReference.cases.length, 24);
    assert.deepEqual([...new Set(emfReference.cases.map(frame => frame.kind))].sort(),
      ['basic','compound','empty','wide-cubic']);
    for (const [index, frame] of emfReference.cases.entries()) {
      const filename = join(directory, 'document with spaces.json'), document = JSON.stringify(frame.input);
      writeFileSync(filename, document);
      const run = spawnSync(process.execPath, [cli,'export-emf',index % 2 ? filename : '-'], {
        input:index % 2 ? undefined : document });
      assert.equal(run.status, 0, run.stderr.toString()); assert.equal(run.stderr.length, 0);
      assert.deepEqual(run.stdout, Buffer.from(frame.expected, 'latin1'));
    }
  } finally { rmSync(directory, { recursive:true, force:true }); }
});

test('CLI reports EMF device/graph errors without partial binary output and lists the command', () => {
  for (const input of [{}, { ...emfReference.cases[0].input, referenceDevice:undefined },
    { ...emfReference.cases[0].input, referenceDevice:{ pixels:[0,1] } }]) {
    const run = spawnSync(process.execPath, [cli,'export-emf','-'], { input:JSON.stringify(input) });
    assert.equal(run.status, 1); assert.equal(run.stdout.length, 0); assert.match(run.stderr.toString(), /EMF/);
  }
  const help = spawnSync(process.execPath, [cli,'--help'], { encoding:'utf8' });
  assert.equal(help.status, 0); assert.equal(help.stderr, ''); assert.match(help.stdout, /export-emf/);
});

test('CLI writes all 38 exact DXF cases with distinct curve modes, full palettes and hidden extremes', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-dxf-'));
  try {
    assert.equal(dxfReference.cases.length, 38); assert.equal(dxfReference.distinctCurveModes, true);
    for (const [index, fixture] of dxfReference.cases.entries()) {
      const filename = join(directory, 'document with spaces.json');
      writeFileSync(filename, JSON.stringify(fixture.input));
      const run = spawnSync(process.execPath, [cli, 'export-dxf', index % 2 ? filename : '-'], {
        encoding:'utf8', input:index % 2 ? undefined : JSON.stringify(fixture.input) });
      assert.equal(run.status, 0, run.stderr); assert.equal(run.stderr, ''); assert.equal(run.stdout, fixture.expected);
    }
  } finally { rmSync(directory, { recursive:true }); }
});

test('CLI documents DXF and reports missing palettes, invalid modes and overlong fields without partial output', () => {
  for (const document of [{}, { ...dxfReference.cases[0].input, palette:undefined },
    { ...dxfReference.cases[0].input, curveMode:'unknown' }, dxfOverlong.cases[0].input]) {
    const run = spawnSync(process.execPath, [cli, 'export-dxf', '-'], { encoding:'utf8', input:JSON.stringify(document) });
    assert.equal(run.status, 1); assert.equal(run.stdout, ''); assert.match(run.stderr, /DXF/);
  }
  const help = spawnSync(process.execPath, [cli, 'export-dxf', '--help'], { encoding:'utf8' });
  assert.equal(help.status, 0); assert.equal(help.stderr, ''); assert.match(help.stdout, /export-dxf/);
});

test('CLI writes exact original AI bytes for all layouts and strokes from stdin or a file', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-ai-'));
  try {
    assert.equal(aiReference.cases.length, 6);
    for (const [index, fixture] of aiReference.cases.entries()) {
      const filename = join(directory, 'document.json');
      writeFileSync(filename, JSON.stringify(fixture.input));
      const run = spawnSync(process.execPath, [cli, 'export-ai', index % 2 ? filename : '-'], {
        encoding:'utf8', input:index % 2 ? undefined : JSON.stringify(fixture.input) });
      assert.equal(run.status, 0, run.stderr); assert.equal(run.stderr, '');
      assert.equal(run.stdout, fixture.expected);
    }
  } finally { rmSync(directory, { recursive:true }); }
});

test('CLI documents AI syntax and rejects invalid input without partial output', () => {
  for (const document of [{}, { ...aiReference.cases[0].input, layout:'unknown' }]) {
    const run = spawnSync(process.execPath, [cli, 'export-ai', '-'], { encoding:'utf8', input:JSON.stringify(document) });
    assert.equal(run.status, 1); assert.equal(run.stdout, ''); assert.match(run.stderr, /AI/);
  }
  const help = spawnSync(process.execPath, [cli, 'export-ai', '--help'], { encoding:'utf8' });
  assert.equal(help.status, 0); assert.equal(help.stderr, ''); assert.match(help.stdout, /export-ai/);
});

test('CLI writes exact binary PDF bytes for all layouts and strokes from stdin or a file', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-pdf-'));
  try {
    assert.equal(pdfReference.cases.length, 6);
    for (const [index, fixture] of pdfReference.cases.entries()) {
      const filename = join(directory, 'document.json');
      writeFileSync(filename, JSON.stringify(fixture.input));
      const run = spawnSync(process.execPath, [cli, 'export-pdf', index % 2 ? filename : '-'], {
        input:index % 2 ? undefined : JSON.stringify(fixture.input) });
      assert.equal(run.status, 0, run.stderr.toString()); assert.equal(run.stderr.length, 0);
      assert.deepEqual(run.stdout, Buffer.from(fixture.expected, 'latin1'));
    }
  } finally { rmSync(directory, { recursive:true }); }
});

test('CLI documents PDF syntax and rejects invalid input without emitting a binary header', () => {
  for (const document of [{}, { ...pdfReference.cases[0].input, layout:'unknown' }]) {
    const run = spawnSync(process.execPath, [cli, 'export-pdf', '-'], { input:JSON.stringify(document) });
    assert.equal(run.status, 1); assert.equal(run.stdout.length, 0); assert.match(run.stderr.toString(), /PDF/);
  }
  const help = spawnSync(process.execPath, [cli, 'export-pdf', '--help'], { encoding:'utf8' });
  assert.equal(help.status, 0); assert.equal(help.stderr, ''); assert.match(help.stdout, /export-pdf/);
});

test('CLI writes exact original SVG bytes for all layouts and stroke choices from stdin or a file', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-svg-'));
  try {
    for (let index = 0; index < 6; index++) {
      const fixture = svgReference.cases[index];
      const document = fixture.input;
      const filename = join(directory, 'document.json');
      writeFileSync(filename, JSON.stringify(document));
      const run = spawnSync(process.execPath, [cli, 'export-svg', index % 2 ? filename : '-'], {
        encoding:'utf8', input:index % 2 ? undefined : JSON.stringify(document) });
      assert.equal(run.status, 0, run.stderr); assert.equal(run.stderr, '');
      assert.equal(run.stdout, fixture.expected);
    }
  } finally { rmSync(directory, { recursive:true }); }
});

test('CLI documents SVG syntax and rejects bad export input without partial SVG', () => {
  for (const document of [{}, { ...svgReference.cases[0].input, layout:'unknown' }]) {
    const run = spawnSync(process.execPath, [cli, 'export-svg', '-'], { encoding:'utf8', input:JSON.stringify(document) });
    assert.equal(run.status, 1); assert.equal(run.stdout, ''); assert.match(run.stderr, /SVG/);
  }
  const help = spawnSync(process.execPath, [cli, 'export-svg', '--help'], { encoding:'utf8' });
  assert.equal(help.status, 0); assert.equal(help.stderr, ''); assert.match(help.stdout, /export-svg/);
});

test('CLI writes exact EPS bytes for every layout, stroke and operator style from stdin or a file', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-eps-'));
  try {
    assert.equal(epsReference.cases.length, 12);
    for (const [index, fixture] of epsReference.cases.entries()) {
      const filename = join(directory, 'document.json');
      writeFileSync(filename, JSON.stringify(fixture.input));
      const run = spawnSync(process.execPath, [cli, 'export-eps', index % 2 ? filename : '-'], {
        encoding:'utf8', input:index % 2 ? undefined : JSON.stringify(fixture.input) });
      assert.equal(run.status, 0, run.stderr); assert.equal(run.stderr, '');
      assert.equal(run.stdout, fixture.expected);
    }
  } finally { rmSync(directory, { recursive:true }); }
});

test('CLI documents EPS syntax and rejects invalid EPS input without partial bytes', () => {
  for (const document of [{}, { ...epsReference.cases[0].input, compact:1 },
    { ...epsReference.cases[0].input, creationDate:'injected\ncommand' }]) {
    const run = spawnSync(process.execPath, [cli, 'export-eps', '-'], { encoding:'utf8', input:JSON.stringify(document) });
    assert.equal(run.status, 1); assert.equal(run.stdout, ''); assert.match(run.stderr, /EPS/);
  }
  const help = spawnSync(process.execPath, [cli, 'export-eps', '--help'], { encoding:'utf8' });
  assert.equal(help.status, 0); assert.equal(help.stderr, ''); assert.match(help.stdout, /export-eps/);
});

test('CLI fits all three prepared basic categories against complete original curve geometry', () => {
  for (const fixture of fittingReference.cases) {
    const run = spawnSync(process.execPath, [cli, 'fit-contours', '-'], { encoding:'utf8', input:JSON.stringify(fixture.input) });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stderr, '');
    assert.deepEqual(JSON.parse(run.stdout), fixture.expected);
  }
});

test('CLI fitting reports invalid inputs without partial output and documents its syntax', () => {
  const input = structuredClone(fittingReference.cases[0].input);
  delete input.settings;
  const missing = spawnSync(process.execPath, [cli, 'fit-contours', '-'], { encoding:'utf8', input:JSON.stringify(input) });
  assert.equal(missing.status, 1); assert.equal(missing.stdout, '');
  assert.equal(missing.stderr, 'Fitting needs a settings object with explicit controls.\n');
  input.settings = fittingReference.cases[0].input.settings;
  input.geometry.regions[0].contour[0] = -1;
  const malformed = spawnSync(process.execPath, [cli, 'fit-contours', '-'], { encoding:'utf8', input:JSON.stringify(input) });
  assert.equal(malformed.status, 1); assert.equal(malformed.stdout, ''); assert.match(malformed.stderr, /Fitting needs/);
  const help = spawnSync(process.execPath, [cli, 'fit-contours', '--help'], { encoding:'utf8' });
  assert.equal(help.status, 0); assert.equal(help.stderr, ''); assert.match(help.stdout, /fit-contours/);
});

test('CLI smooths prepared plain and anti-aliased contours against original full-stage results', () => {
  for (const fixture of smoothingReference.cases) {
    const run = spawnSync(process.execPath, [cli, 'smooth-contours', '-'], { encoding:'utf8', input:JSON.stringify(fixture.input) });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stderr, '');
    assert.deepEqual(JSON.parse(run.stdout), fixture.expected);
  }
});

test('CLI rejects missing smoothing settings without emitting partial JSON', () => {
  const input = structuredClone(smoothingReference.cases[0].input);
  delete input.settings;
  const run = spawnSync(process.execPath, [cli, 'smooth-contours', '-'], { encoding:'utf8', input:JSON.stringify(input) });
  assert.equal(run.status, 1);
  assert.equal(run.stdout, '');
  assert.equal(run.stderr, 'Smoothing needs a settings object with explicit controls.\n');
  const help = spawnSync(process.execPath, [cli, 'smooth-contours', '--help'], { encoding:'utf8' });
  assert.equal(help.status, 0);
  assert.equal(help.stderr, '');
  assert.match(help.stdout, /smooth-contours/);
});

test('CLI accepts a supplied palette for plain and anti-aliased segmentation', () => {
  for (const [profile, category, quality] of [[3, 'artwork-without-blending', 'high'], [6, 'artwork-with-blending', 'high']]) {
    const fixture = suppliedPaletteReference.cases.find(sample => sample.profile === profile && sample.width === 17 && sample.palette.length > 4);
    assert.ok(fixture);
    const run = spawnSync(process.execPath, [cli, 'segment', '-'], { encoding:'utf8',
      input:JSON.stringify({ width:fixture.width, height:fixture.height, bgra:fixture.bgra, palette:fixture.palette,
        settings:{ mode:'basic', category, quality } }) });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stderr, '');
    assert.deepEqual(JSON.parse(run.stdout), fixture.expected);
  }
});
import { fileURLToPath } from 'node:url';
import generationReference from './fixtures/generation-reference.json' with { type: 'json' };
import refinementReference from './fixtures/refinement-reference.json' with { type: 'json' };
import weightedGenerationReference from './fixtures/weighted-generation-reference.json' with { type: 'json' };
import weightedRefinementReference from './fixtures/weighted-refinement-reference.json' with { type: 'json' };
import segmentationReference from './fixtures/unweighted-segmentation-reference.json' with { type: 'json' };
import diagonalSegmentationReference from './fixtures/unweighted-diagonal-segmentation-reference.json' with { type: 'json' };

const cli = fileURLToPath(new URL('../bin/openvector.mjs', import.meta.url));

test('CLI traces original plain and anti-aliased contours with automatic or supplied palettes', () => {
  for (const [profile, category, source, mode] of [
    [3, 'artwork-without-blending', diagonalSegmentationReference, 'unweighted-diagonal-segmentation'],
    [6, 'artwork-with-blending', antiAliasReference, 'anti-alias-segmentation'],
    [3, 'artwork-without-blending', suppliedPaletteReference, 'supplied-palette-segmentation'],
    [6, 'artwork-with-blending', suppliedPaletteReference, 'supplied-palette-segmentation']]) {
    const sourceCase = source.cases.findIndex(fixture => fixture.profile === profile && fixture.width === 17);
    assert.ok(sourceCase >= 0);
    const input = source.cases[sourceCase];
    const fixture = contourReference.cases.find(sample => sample.sourceMode === mode && sample.sourceCase === sourceCase);
    assert.ok(fixture);
    const run = spawnSync(process.execPath, [cli, 'trace-contours', '-'], { encoding:'utf8',
      input:JSON.stringify({ width:input.width, height:input.height, bgra:input.bgra,
        ...(mode === 'supplied-palette-segmentation' ? { palette:input.palette } : {}),
        settings:{ mode:'basic', category, quality:'high' } }) });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stderr, '');
    assert.deepEqual(JSON.parse(run.stdout), fixture.expected);
  }
});

test('CLI reports contour-stage limits without partial JSON and documents the command', () => {
  const run = spawnSync(process.execPath, [cli, 'trace-contours', '-'], { encoding:'utf8',
    input:JSON.stringify({ width:1, height:1, bgra:[0, 0, 0, 255],
      settings:{ mode:'basic', category:'photograph', quality:'high' } }) });
  assert.equal(run.status, 1);
  assert.equal(run.stdout, '');
  assert.equal(run.stderr, 'Contour extraction needs int32 dimensions of at least 2 by 2.\n');
  const help = spawnSync(process.execPath, [cli, 'trace-contours', '--help'], { encoding:'utf8' });
  assert.equal(help.status, 0);
  assert.equal(help.stderr, '');
  assert.match(help.stdout, /trace-contours/);
});

test('CLI accepts JSON over stdin and emits standalone stage parameters', () => {
  const run = spawnSync(process.execPath, [cli, 'settings', '-'], {
    input: JSON.stringify({ mode: 'basic', stage: 'segmentation', category: 'photograph', quality: 'high' }),
    encoding: 'utf8',
  });
  assert.equal(run.status, 0, run.stderr);
  assert.equal(run.stderr, '');
  const parameters = JSON.parse(run.stdout);
  assert.equal(Object.keys(parameters).length, 100);
  assert.equal(parameters['Shared::image_type_code'], 8);
});

test('CLI reports invalid settings without emitting partial JSON', () => {
  const run = spawnSync(process.execPath, [cli, 'settings', '-'], {
    input: JSON.stringify({ mode: 'basic', stage: 'segmentation', category: 'invalid', quality: 'high' }),
    encoding: 'utf8',
  });
  assert.equal(run.status, 1);
  assert.equal(run.stdout, '');
  assert.match(run.stderr, /category must be one of/);
});

test('CLI fills a segmentation region without joining diagonal contacts', () => {
  const run = spawnSync(process.execPath, [cli, 'fill', '-', '0', '7'], {
    input: JSON.stringify({ width: 2, height: 2, segments: [0, 1, 1, 0] }),
    encoding: 'utf8',
  });
  assert.equal(run.status, 0, run.stderr);
  assert.equal(run.stderr, '');
  assert.deepEqual(JSON.parse(run.stdout), { width: 2, height: 2, segments: [7, 1, 1, 0] });
});

test('CLI rejects malformed edit arguments without emitting partial JSON', () => {
  const run = spawnSync(process.execPath, [cli, 'fill', '-', '', '7'], {
    input: JSON.stringify({ width: 1, height: 1, segments: [0] }),
    encoding: 'utf8',
  });
  assert.equal(run.status, 1);
  assert.equal(run.stdout, '');
  assert.match(run.stderr, /seed must be an integer/);
});

test('CLI serializes palette assignments as a JSON array', () => {
  const run = spawnSync(process.execPath, [cli, 'assign-palette', '-'], {
    input: JSON.stringify({ bgra: [0, 0, 0, 255, 255, 255, 255, 255], palette: [0, 0, 0, 1, 1, 1, 1, 1] }),
    encoding: 'utf8',
  });
  assert.equal(run.status, 0, run.stderr);
  assert.equal(run.stderr, '');
  assert.deepEqual(JSON.parse(run.stdout), [0, 1]);
});

test('CLI serializes generated and refined typed arrays from a fresh palette session', () => {
  for (const [command, fixture, antiAlias] of [['generate-palette', generationReference.cases[0], false],
    ['refine-palette', refinementReference.cases[0], false],
    ['generate-palette', weightedGenerationReference.cases[0], true],
    ['refine-palette', weightedRefinementReference.cases[0], true]]) {
    const run = spawnSync(process.execPath, [cli, command, '-'], { input: JSON.stringify({ ...fixture, antiAlias }), encoding: 'utf8' });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stderr, '');
    assert.deepEqual(JSON.parse(run.stdout), {
      palette: fixture.expected.palette,
      indices: fixture.expected.indices,
      weights: fixture.expected.weights,
      passes: fixture.expected.passes,
    });
  }
});

test('CLI requires an explicit palette anti-alias mode', () => {
  const run = spawnSync(process.execPath, [cli, 'generate-palette', '-'], {
    input: JSON.stringify(generationReference.cases[0]), encoding: 'utf8',
  });
  assert.equal(run.status, 1);
  assert.equal(run.stdout, '');
  assert.match(run.stderr, /require antiAlias: true or false/);
});

test('CLI resolves segmentation controls and serializes the native region result', () => {
  for (const [reference, profile, category, quality] of [[segmentationReference, 9, 'photograph', 'high'],
    [segmentationReference, 8, 'photograph', 'medium'],
    [segmentationReference, 7, 'photograph', 'low'],
    [diagonalSegmentationReference, 1, 'artwork-without-blending', 'low'],
    [diagonalSegmentationReference, 2, 'artwork-without-blending', 'medium'],
    [diagonalSegmentationReference, 3, 'artwork-without-blending', 'high'],
    [antiAliasReference, 4, 'artwork-with-blending', 'low'],
    [antiAliasReference, 5, 'artwork-with-blending', 'medium'],
    [antiAliasReference, 6, 'artwork-with-blending', 'high']]) {
    const fixture = reference.cases.filter(sample => sample.profile === profile && sample.width === 17).at(-1);
    const run = spawnSync(process.execPath, [cli, 'segment', '-'], {
      input: JSON.stringify({ width: fixture.width, height: fixture.height, bgra: fixture.bgra,
        settings: { mode: 'basic', category, quality } }), encoding: 'utf8',
    });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stderr, '');
    assert.deepEqual(JSON.parse(run.stdout), fixture.expected);
  }
});

test('CLI rejects malformed segmentation controls without emitting partial JSON', () => {
  const run = spawnSync(process.execPath, [cli, 'segment', '-'], {
    input: JSON.stringify({ width: 1, height: 1, bgra: [0, 0, 0, 255],
      settings: { mode: 'advanced', complexity: 6, minimumPixels: 0,
        artifactRejection: 'moderate', clusterColors: 'yes' } }), encoding: 'utf8',
  });
  assert.equal(run.status, 1);
  assert.equal(run.stdout, '');
  assert.equal(run.stderr, 'clusterColors must be true or false.\n');
});

test('CLI exposes native advanced color clustering at three complexity levels', () => {
  for (const complexity of [1, 6, 12]) {
    const settings = { mode:'advanced', complexity, minimumPixels:2, artifactRejection:'moderate', clusterColors:true };
    const parameters = resolveParameters({ ...settings, stage:'segmentation' });
    const fixture = clusteredReference.cases.find(input => input.profile === null && input.width === 17 &&
      input.parameters['Segmenter::min_num_pixels'] === 2 && input.parameters['Segmenter::use_over_seg_diag_extr'] === 0 &&
      input.parameters['Segmenter::lambda_final'] === parameters['Segmenter::lambda_final']);
    assert.ok(fixture);
    const run = spawnSync(process.execPath, [cli,'segment','-'], { encoding:'utf8',
      input:JSON.stringify({ width:fixture.width,height:fixture.height,bgra:fixture.bgra,settings }) });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stderr, '');
    assert.deepEqual(JSON.parse(run.stdout), fixture.expected);
  }
});

test('CLI forwards native shadow context for advanced segmentation', async () => {
  const stream = createReadStream(new URL('./fixtures/document-segmentation-reference.ndjson.gz',import.meta.url));
  const decoder = createGunzip(),rows = createInterface({input:stream.pipe(decoder),crlfDelay:Infinity});
  let frame;
  try {
    for await (const line of rows) {
      const candidate = JSON.parse(line);
      if (candidate.id === '31x27-noisy') {frame = candidate;break;}
    }
  } finally {rows.close();stream.destroy();decoder.destroy();}
  assert.ok(frame);
  const index = frame.steps.findIndex(step => step.id === 'advanced-0-0');assert.ok(index >= 0);
  const step = frame.steps[index],native = frame.states[index].state;
  const [width,height] = native.workingDimensions;
  const input = {width,height,bgra:Array.from(Buffer.from(native.workingBgra,'base64')),
    settings:{mode:'advanced',complexity:step.complexity,minimumPixels:step.minimumPixels,
      artifactRejection:'none',clusterColors:step.clusterColors},shadowControl:native.shadowControl};
  const run = spawnSync(process.execPath,[cli,'segment','-'],{input:JSON.stringify(input),encoding:'utf8'});
  assert.equal(run.status,0,run.stderr);assert.equal(run.stderr,'');
  const output = JSON.parse(run.stdout),reference = native.segmentation;
  const indices = Buffer.from(reference.indexBits,'hex'),palette = Buffer.from(reference.paletteBits,'hex');
  assert.deepEqual(output,{indices:Array.from({length:indices.length/4},(_,item) => indices.readInt32LE(item*4)),
    palette:Array.from({length:palette.length/4},(_,item) => palette.readFloatLE(item*4)),
    counts:reference.counts,colorIndices:reference.colorIndices});
  delete input.shadowControl;
  const missing = spawnSync(process.execPath,[cli,'segment','-'],{input:JSON.stringify(input),encoding:'utf8'});
  assert.equal(missing.status,1);assert.equal(missing.stdout,'');
  assert.equal(missing.stderr,'Enabled shadowing requires an explicit int32 shadowControl.\n');
});
