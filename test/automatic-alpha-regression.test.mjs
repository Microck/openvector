import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const require = createRequire(import.meta.url);
const native = process.env.OPENVECTOR_NODE_BINARY
  ? require(process.env.OPENVECTOR_NODE_BINARY)
  : await import('../npm/index.mjs');

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const name = Buffer.from(type), length = Buffer.alloc(4), checksum = Buffer.alloc(4);
  length.writeUInt32BE(data.length); checksum.writeUInt32BE(crc32(Buffer.concat([name, data])));
  return Buffer.concat([length, name, data, checksum]);
}
function fixture() {
  const width = 48, height = 48, header = Buffer.alloc(13);
  header.writeUInt32BE(width); header.writeUInt32BE(height, 4);
  header[8] = 8; header[9] = 6;
  const raw = Buffer.alloc(height * (1 + width * 4));
  for (let y = 8; y < 40; y++) for (let x = 8; x < 40; x++) {
    const index = y * (1 + width * 4) + 1 + x * 4;
    raw[index] = 255; raw[index + 3] = 128;
  }
  return Buffer.concat([Buffer.from('89504e470d0a1a0a', 'hex'), chunk('IHDR', header),
    chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}
test('automatic transparent red remains straight RGB in SVG', () => {
  const document = new native.CoreSession().createDocument();
  document.setSource(fixture(), 'transparent-red.png');
  document.runAutomatic({extension: '.png'});
  const dir = mkdtempSync(join(tmpdir(), 'openvector-alpha-'));
  try {
    const path = join(dir, 'output.svg');
    assert.equal(document.saveVector(path, {layout: 'stacked'}).status, 0);
    const svg = readFileSync(path, 'utf8');
    assert.match(svg, /fill="#ff0000"[^>]*opacity="0[.]5/i);
    assert.throws(() => document.runAutomatic({}), /extension/i);
    const afterRejected = join(dir, 'after-rejected.svg');
    assert.equal(document.saveVector(afterRejected, {layout: 'stacked'}).status, 0);
    assert.match(readFileSync(afterRejected, 'utf8'), /fill="#ff0000"[^>]*opacity="0[.]5/i);
    const dxf = join(dir, 'output.dxf');
    assert.equal(document.saveVector(dxf, {layout: 'grouped-cutouts'}).status, 0);
    const layers = readFileSync(dxf, 'utf8');
    assert.match(layers, /color#ff000080/i);
    assert.doesNotMatch(layers, /color#80000080/i);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('a local edit does not darken untouched automatic regions', () => {
  const document = new native.CoreSession().createDocument();
  document.setSource(fixture(), 'transparent-red.png');
  const {settings} = document.runAutomatic({extension: '.png'});
  document.beginEditing();
  const before = document.editing();
  const untouched = (20 * before.width + 20) * 4;
  assert.deepEqual(Array.from(before.bgra.subarray(untouched, untouched + 4)), [0, 0, 255, 128]);
  document.edit({tool: 'pencil', from: [10, 10], to: [14, 10], color: [0, 0.25, 0.8, 0.5]});
  const painted = document.editing().bgra;
  document.undoEdit();
  assert.deepEqual(document.editing().bgra, before.bgra);
  document.redoEdit();
  assert.deepEqual(document.editing().bgra, painted);
  document.finishEditing(settings);
  const dir = mkdtempSync(join(tmpdir(), 'openvector-alpha-edit-'));
  try {
    const path = join(dir, 'output.svg');
    assert.equal(document.saveVector(path, {layout: 'stacked'}).status, 0);
    const svg = readFileSync(path, 'utf8');
    assert.match(svg, /fill="#ff0000"[^>]*opacity="0[.]5/i);
    assert.match(svg, /fill="#cc4000"[^>]*opacity="0[.]5/i);
    const editedDxf = join(dir, 'edited.dxf');
    assert.equal(document.saveVector(editedDxf, {layout: 'grouped-cutouts'}).status, 0);
    const dxf = readFileSync(editedDxf, 'utf8');
    assert.match(dxf, /color#ff000080/i);
    assert.match(dxf, /color#cc400080/i);
    assert.doesNotMatch(dxf, /color#80000080/i);
    const modified = {...settings, quality: (settings.quality + 1) % 3};
    for (const stage of ['segmentation', 'smoothing', 'fitting']) document.runTask(stage, modified);
    const reprocessed = join(dir, 'reprocessed.svg');
    assert.equal(document.saveVector(reprocessed, {layout: 'stacked'}).status, 0);
    assert.match(readFileSync(reprocessed, 'utf8'), /fill="#ff0000"[^>]*opacity="0[.]5/i);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('automatic colors survive segmentation reprocessing', () => {
  const document = new native.CoreSession().createDocument();
  document.setSource(fixture(), 'transparent-red.png');
  const {settings} = document.runAutomatic({extension: '.png'});
  const other = {...settings, palette: settings.palette.map((value, index) => index === 0 ? 1 - value : value)};
  document.commitSettings(other);
  document.commitSettings(settings);
  const modified = {...settings, quality: (settings.quality + 1) % 3};
  for (const stage of ['segmentation', 'smoothing', 'fitting']) document.runTask(stage, modified);
  const dir = mkdtempSync(join(tmpdir(), 'openvector-alpha-reprocess-'));
  try {
    const path = join(dir, 'output.svg');
    assert.equal(document.saveVector(path, {layout: 'stacked'}).status, 0);
    assert.match(readFileSync(path, 'utf8'), /fill="#ff0000"[^>]*opacity="0[.]5/i);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('resuming after smoothing cancellation keeps automatic color encoding', () => {
  const document = new native.CoreSession().createDocument();
  document.setSource(fixture(), 'transparent-red.png');
  let controls;
  assert.throws(() => document.runAutomatic({extension: '.png'}, {
    callback(event, state) {
      if (event.stage === 'smoothing') { controls = state.settings; return false; }
      return true;
    }
  }), /cancel/i);
  assert.ok(controls, 'smoothing must have started');
  document.runTask('smoothing', controls);
  document.runTask('fitting', controls);
  const dir = mkdtempSync(join(tmpdir(), 'openvector-alpha-resume-'));
  try {
    const path = join(dir, 'resumed.svg');
    assert.equal(document.saveVector(path, {layout: 'stacked'}).status, 0);
    assert.match(readFileSync(path, 'utf8'), /fill="#ff0000"[^>]*opacity="0[.]5/i);
    document.beginEditing();
    const editor = document.editing();
    const offset = (20 * editor.width + 20) * 4;
    assert.deepEqual(Array.from(editor.bgra.subarray(offset, offset + 4)), [0, 0, 255, 128]);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});

test('a committed custom translucent palette remains straight alpha', () => {
  const document = new native.CoreSession().createDocument();
  document.setSource(fixture(), 'transparent-red.png');
  const {settings} = document.runAutomatic({extension: '.png'});
  const palette = [...settings.palette];
  for (let index = 0; index < palette.length; index += 4) {
    if (palette[index + 3] > 0) palette[index + 2] = 0.8;
  }
  const custom = {...settings, palette};
  document.commitSettings(custom);
  for (const stage of ['segmentation', 'smoothing', 'fitting']) document.runTask(stage, custom);
  const dir = mkdtempSync(join(tmpdir(), 'openvector-alpha-custom-'));
  try {
    const path = join(dir, 'custom.svg');
    assert.equal(document.saveVector(path, {layout: 'stacked'}).status, 0);
    assert.match(readFileSync(path, 'utf8'), /fill="#cc0000"[^>]*opacity="0[.]5/i);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});


test('resuming after segmentation cancellation keeps automatic color encoding', () => {
  const baseline = new native.CoreSession().createDocument();
  baseline.setSource(fixture(), 'transparent-red.png');
  const {settings} = baseline.runAutomatic({extension: '.png'});
  const document = new native.CoreSession().createDocument();
  document.setSource(fixture(), 'transparent-red.png');
  let interrupted = false;
  assert.throws(() => document.runAutomatic({extension: '.png'}, {
    callback(event) {
      if (event.stage === 'segmentation') { interrupted = true; return false; }
      return true;
    }
  }), /cancel/i);
  assert.ok(interrupted, 'segmentation must have started');
  for (const stage of ['segmentation', 'smoothing', 'fitting']) document.runTask(stage, settings);
  const dir = mkdtempSync(join(tmpdir(), 'openvector-alpha-seg-resume-'));
  try {
    const path = join(dir, 'resumed.svg');
    assert.equal(document.saveVector(path, {layout: 'stacked'}).status, 0);
    assert.match(readFileSync(path, 'utf8'), /fill="#ff0000"[^>]*opacity="0[.]5/i);
    document.beginEditing();
    const editor = document.editing();
    const offset = (20 * editor.width + 20) * 4;
    assert.deepEqual(Array.from(editor.bgra.subarray(offset, offset + 4)), [0, 0, 255, 128]);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});


test('a manually authored palette identical to a generated palette can stay straight', () => {
  const document = new native.CoreSession().createDocument();
  document.setSource(fixture(), 'transparent-red.png');
  const {settings} = document.runAutomatic({extension: '.png'});
  const dir = mkdtempSync(join(tmpdir(), 'openvector-alpha-explicit-'));
  try {
    const straight = {...settings, paletteEncoding: 'straight'};
    for (const stage of ['segmentation', 'smoothing', 'fitting']) document.runTask(stage, straight);
    const path = join(dir, 'straight.svg');
    assert.equal(document.saveVector(path, {layout: 'stacked'}).status, 0);
    assert.match(readFileSync(path, 'utf8'), /fill="#800000"[^>]*opacity="0[.]5/i);
    const generated = {...settings, paletteEncoding: 'premultiplied'};
    for (const stage of ['segmentation', 'smoothing', 'fitting']) document.runTask(stage, generated);
    const restored = join(dir, 'generated.svg');
    assert.equal(document.saveVector(restored, {layout: 'stacked'}).status, 0);
    assert.match(readFileSync(restored, 'utf8'), /fill="#ff0000"[^>]*opacity="0[.]5/i);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});


test('explicit generated encoding preserves edits on a segmentation cache hit', () => {
  const document = new native.CoreSession().createDocument();
  document.setSource(fixture(), 'transparent-red.png');
  const {settings} = document.runAutomatic({extension: '.png'});
  document.beginEditing();
  document.edit({tool: 'pencil', from: [10, 10], to: [14, 10], color: [0, 0.25, 0.8, 0.5]});
  document.finishEditing(settings);
  const before = document.snapshot({});
  assert.ok(before.stageKeys.segmentationEdits);
  const hit = document.runTask('segmentation', {...settings, paletteEncoding: 'premultiplied'});
  assert.equal(hit.didWork, false);
  assert.equal(document.snapshot({}).stageKeys.segmentationEdits, before.stageKeys.segmentationEdits);
});

test('explicit generated colors produce matching grouped DXF layers', () => {
  const baseline = new native.CoreSession().createDocument();
  baseline.setSource(fixture(), 'transparent-red.png');
  const {settings} = baseline.runAutomatic({extension: '.png'});
  const document = new native.CoreSession().createDocument();
  document.setSource(fixture(), 'transparent-red.png');
  const explicit = {...settings, paletteEncoding: 'premultiplied'};
  for (const stage of ['segmentation', 'smoothing', 'fitting']) document.runTask(stage, explicit);
  const dir = mkdtempSync(join(tmpdir(), 'openvector-alpha-explicit-dxf-'));
  try {
    const path = join(dir, 'output.dxf');
    assert.equal(document.saveVector(path, {layout: 'grouped-cutouts'}).status, 0);
    const dxf = readFileSync(path, 'utf8');
    assert.match(dxf, /color#ff000080/i);
    assert.doesNotMatch(dxf, /color#80000080/i);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});
