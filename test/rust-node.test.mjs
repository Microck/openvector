import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { existsSync, readFileSync, readdirSync, mkdtempSync, mkdirSync, statSync, writeFileSync, rmSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const require = createRequire(import.meta.url);
const native = process.env.OPENVECTOR_NODE_BINARY
  ? require(process.env.OPENVECTOR_NODE_BINARY)
  : await import('../npm/index.mjs');
const fixtureDirectory = process.env.OPENVECTOR_FIXTURES;
function fixture(name) {
  const path = fixtureDirectory ? `${fixtureDirectory}/${name}` : new URL(`./fixtures/${name}`, import.meta.url);
  return gunzipSync(readFileSync(path)).toString().trim().split('\n').map(JSON.parse);
}

test('Rust Node binding preserves native file-size labels', () => {
  let comparisons = 0;
  for (const reference of fixture('source-history-reference.ndjson.gz').slice(1)) {
    for (const [index, action] of reference.actions.entries()) {
      if (action[0] !== 'describe-size') continue;
      assert.equal(native.formatFileSize(action[1]), reference.expected[index].sizeDescription.text);
      comparisons++;
    }
  }
  assert.equal(comparisons, 24);
});

test('Rust history owner rejects invalid changes without losing prior state', () => {
  const history = new native.SourceHistory();
  history.add('missing naïve.bmp', 13, 7, 334);
  const expected = { capacity: 3, files: [{ filename: 'missing naïve.bmp', description: '13x7, 334.0 B', thumbnailDimensions: [32, 17] }] };
  assert.deepEqual(history.snapshot(), expected);
  for (const [width, height, bytes] of [[0, 7, 334], [13, 1.5, 334], [13, 7, -1], [13, 7, Number.MAX_SAFE_INTEGER + 1]]) {
    assert.throws(() => history.add('other.bmp', width, height, bytes));
    assert.deepEqual(history.snapshot(), expected);
  }
  const restored = new native.SourceHistory(history.snapshot());
  history.clear();
  assert.deepEqual(restored.snapshot(), expected);
  restored.remove('missing naïve.bmp');
  assert.deepEqual(restored.snapshot(), { capacity: 3, files: [] });
});

test('Rust directory owner retains case and survives file-history clearing', () => {
  const directories = new native.SourceDirectories();
  directories.add('D:/naïve');
  directories.add('D:/NAÏVE');
  directories.add('D:/third');
  directories.add('D:/fourth');
  directories.add('D:/fifth');
  assert.deepEqual(directories.snapshot(), { capacity: 4, directories: ['D:/fifth', 'D:/fourth', 'D:/third', 'D:/NAÏVE'] });
  const restored = new native.SourceDirectories(directories.snapshot());
  assert.throws(() => restored.add(''));
  assert.deepEqual(restored.snapshot(), directories.snapshot());
  directories.clear();
  assert.deepEqual(restored.snapshot().directories, ['D:/fifth', 'D:/fourth', 'D:/third', 'D:/NAÏVE']);
});

test('Node document decodes, processes and exports native scene bytes through Rust', () => {
  const scenes = fixture('bitmap-scenes-reference.ndjson.gz');
  for (const id of ['opaque-four-plugins', 'alpha-png', 'fraction']) {
    const scene = scenes.find(row => row.id === id);
    const session = new native.CoreSession();
    const document = session.createDocument();
    document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
    const extension = `.${scene.sourceFilename.split('.').at(-1)}`;
    const processed = document.runAutomatic({ extension });
    assert.equal(processed.state.fittingComplete, true, id);
    for (const [filename, expected] of Object.entries(scene.expected)) {
      const format = filename.split('.').at(-1);
      assert.deepEqual(document.exportBitmap(format, scene.scale, false), Buffer.from(expected.bytes, 'base64'), `${id}: ${filename}`);
    }
    if (id === 'alpha-png') {
      assert.throws(() => document.exportBitmap('bmp'), error => error.code === 'BITMAP_FLATTEN_REQUIRED');
      const flattened = native.decodeImage(document.exportBitmap('bmp', 1, true));
      for (let offset = 3; offset < flattened.bgra.length; offset += 4) assert.equal(flattened.bgra[offset], 255);
    }
  }
});

test('Node failed source replacement preserves the processed document and exports', () => {
  const scene = fixture('bitmap-scenes-reference.ndjson.gz').find(row => row.id === 'opaque-four-plugins');
  const document = new native.CoreSession().createDocument();
  document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
  document.runAutomatic({ extension: '.bmp' });
  const before = document.snapshot();
  const bitmap = document.exportBitmap('png');

  for (const encoded of [Buffer.alloc(0), Buffer.from('not an image'), Buffer.from('89504e470d0a1a0a', 'hex')]) {
    assert.throws(() => document.setSource(encoded, 'replacement.png'));
    assert.deepEqual(document.snapshot(), before);
    assert.deepEqual(document.exportBitmap('png'), bitmap);
  }
});

test('Node progress cancellation retains a retryable Rust document', () => {
  const scene = fixture('bitmap-scenes-reference.ndjson.gz').find(row => row.id === 'opaque-four-plugins');
  const document = new native.CoreSession().createDocument();
  document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
  assert.throws(() => document.runAutomatic({ extension: '.bmp' }, { callback: () => false }), /cancelled/);
  assert.equal(document.snapshot().cancelled, true);
  assert.throws(() => document.exportBitmap('png'), error => error.code === 'GenericFailure');
  const retried = document.runAutomatic({ extension: '.bmp' });
  assert.equal(retried.state.fittingComplete, true);
  assert.deepEqual(document.exportBitmap('png'), Buffer.from(scene.expected['result.png'].bytes, 'base64'));
});

test('Node editing checkpoints distinguish commit, restore, leave and full finish', () => {
  const scene = fixture('bitmap-scenes-reference.ndjson.gz').find(row => row.id === 'opaque-four-plugins');
  // Own the session and compare retained state, not a process-global PRNG reference stream.
  const document = new native.CoreSession().createDocument();
  assert.deepEqual(document.editingHistory(), [0, 0]);
  assert.equal(document.restoreCommittedEditing(), null);
  for (const operation of ['leaveEditing', 'commitEditing']) {
    assert.throws(() => document[operation](), /Call beginEditing/);
  }
  document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
  const { settings, state: processed } = document.runAutomatic({ extension: '.bmp' });
  assert.equal(processed.fittingComplete, true);
  document.beginEditing();
  const initial = document.editing();
  const firstColor = Array.from(initial.bgra.subarray(0, 3), channel => channel < 128 ? 1 : 0).concat(1);
  const secondColor = firstColor.map((channel, index) => index === 3 ? 1 : 1 - channel);
  const paint = color => {
    document.edit({ tool: 'fill', x: 0, y: 0, color });
    return document.editing();
  };

  paint(firstColor);
  const second = paint(secondColor);
  document.undoEdit();
  const checkpoint = document.editing();
  assert.deepEqual(document.editingHistory(), [1, 1]);
  assert.notDeepEqual(checkpoint.bgra, initial.bgra);
  const committed = document.commitEditing();
  assert.equal(committed.stageKeys.segmentationEdits, checkpoint.key);
  for (const stage of ['contours', 'smoothing', 'fitting']) {
    assert.equal(document.snapshot({ contours: true, smoothing: true, fitting: true })[stage], null,
      `commit invalidates ${stage} without refitting`);
  }
  for (const stage of ['smoothing', 'fitting']) {
    assert.equal(committed.stageKeys[stage], processed.stageKeys[stage], 'commit retains old cache keys');
  }

  document.gesture({ phase: 'select', tool: 'pencil' });
  document.redoEdit();
  assert.deepEqual(document.editing().bgra, second.bgra);
  assert.deepEqual(document.editingHistory(), [2, 0]);
  document.restoreCommittedEditing();
  const restored = document.editing();
  assert.deepEqual(restored, { ...checkpoint, tool: 'pencil' });
  assert.deepEqual(document.editingHistory(), [1, 1]);
  assert.deepEqual(document.snapshot({}), committed, 'restore changes only editor history and pixels');

  document.redoEdit();
  const pending = document.editing();
  assert.equal(document.leaveEditing(), undefined);
  document.beginEditing();
  assert.deepEqual(document.editing(), { ...pending, tool: 'pan' });
  assert.deepEqual(document.snapshot({}), committed, 'leave neither commits nor runs vector tasks');
  document.restoreCommittedEditing();
  assert.deepEqual(document.editing(), checkpoint, 'leave preserves the earlier checkpoint');

  const stages = [];
  const { fittingCompletion, ...finished } = document.finishEditing(settings, { callback: event => {
    stages.push(event.stage);
    return true;
  } });
  assert.deepEqual(fittingCompletion, { status: 0, didWork: true }, 'the committed edit key refits');
  assert.ok(stages.includes('smoothing'));
  assert.ok(stages.includes('fitting'));
  assert.equal(finished.fittingComplete, true);
  assert.equal(finished.stageKeys.segmentationEdits, checkpoint.key);
  assert.deepEqual(document.editingHistory(), [1, 1]);

  // The same active key can have a different redo checkpoint. Commit must save it
  // without invalidating the already completed vectors.
  const replacementColor = [0.25, 0.5, 0.75, 1];
  const replacement = paint(replacementColor);
  document.undoEdit();
  assert.deepEqual(document.commitEditing(), finished);
  document.redoEdit();
  document.restoreCommittedEditing();
  const unchanged = document.editing();
  assert.deepEqual(unchanged.bgra, checkpoint.bgra);
  assert.deepEqual(unchanged.undo, checkpoint.undo);
  assert.deepEqual(unchanged.redo, replacement.undo.slice(-1));
  document.redoEdit();
  assert.deepEqual(document.editing().bgra, replacement.bgra);

  document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
  assert.deepEqual(document.editingHistory(), [0, 0]);
  assert.equal(document.restoreCommittedEditing(), null, 'source replacement clears the checkpoint');
});

test('Node batch owner preserves load failures and retries overwrite admission', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-node-batch-'));
  try {
    const scene = fixture('bitmap-scenes-reference.ndjson.gz').find(row => row.id === 'opaque-four-plugins');
    const source = join(directory, 'source.bmp');
    writeFileSync(source, Buffer.from(scene.sourceBytes, 'base64'));
    const batch = new native.CoreSession().createBatch(directory, directory);
    assert.deepEqual(batch.addFiles([join(directory, 'missing.bmp'), source, source]), [source]);
    batch.setExport(directory, 'svg');
    const settings = { advanced: false, category: 3, quality: 3, paletteMode: 3, colorCount: 0, backgroundMode: 3, palette: [] };
    const first = batch.run(settings, {});
    assert.equal(first.failed, 1);
    assert.equal(first.converted, 1);
    assert.deepEqual(first.files.map(file => file.status), ['failed-load', 'written']);
    const second = batch.run(settings, {}, () => 'skip');
    assert.equal(second.skipped, 1);
    assert.equal(second.converted, 0);
    assert.equal(second.failed, 1);
    assert.throws(() => batch.run(settings, {}, () => 'invalid'), /overwrite decision/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('Node saved profiles preserve native rename and storage semantics', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-node-profiles-'));
  try {
    const profiles = new native.SavedAdvancedProfiles();
    const profile = native.defaultAdvancedProfile();
    profiles.put('first', profile);
    profiles.put('second', profile);
    assert.throws(() => profiles.rename('first', 'second', false), /Confirm replacement/);
    assert.equal(profiles.rename('first', 'second', true).changed, true);
    profiles.save(join(directory, 'profiles.json'));
    const restored = new native.SavedAdvancedProfiles();
    restored.load(join(directory, 'profiles.json'));
    assert.equal(restored.snapshot().filter(entry => entry.name === 'second').length, 1);
    assert.notEqual(restored.select('second').index, -1);
    assert.equal(restored.select('missing').profile, null);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('Node palette edits retain separate count metadata and revert ownership', () => {
  const chosen = native.applyPaletteCommand(native.defaultAdvancedProfile(), { backup: [], customized: false },
    { action: 'choose', palette: [0, 0, 0, 1, 1, 1, 1, 1] });
  const deleted = native.applyPaletteCommand(chosen.profile, chosen.runtime, { action: 'delete', index: 0 });
  assert.equal(deleted.profile.paletteColorCount, 2);
  assert.deepEqual(deleted.profile.settings.palette, [1, 1, 1, 1]);
  assert.deepEqual(deleted.runtime.backup, chosen.profile.settings.palette);
  assert.equal(deleted.runtime.customized, true);
  const restored = native.applyPaletteCommand(deleted.profile, deleted.runtime, { action: 'revert' });
  assert.deepEqual(restored.profile.settings.palette, chosen.profile.settings.palette);
  assert.equal(restored.runtime.customized, false);
  assert.throws(() => native.applyPaletteCommand(restored.profile, restored.runtime, { action: 'count', count: 25 }), /2 through 24/);
});

// Basic profiles vary only the label inputs unless a test overrides other controls.
function basicProfile(category, quality, paletteMode, paletteColorCount, settings = {}) {
  const profile = native.defaultAdvancedProfile();
  return { ...profile, paletteColorCount,
    settings: { ...profile.settings, advanced: false, category, quality, paletteMode, ...settings } };
}

test('Node recent basic profiles keep Rust capacity, duplicate and eligibility rules', () => {
  const recorded = [
    [basicProfile(2, 0, 0, 2), 'Artwork, High, Two colors'],
    [basicProfile(0, 1, 1, 8), 'Photograph, Medium, 8 colors'],
    [basicProfile(1, 2, 2, 0), 'Anti-aliased artwork, Low, Unlimited colors'],
    [basicProfile(-1, 1, 1, 2), 'Medium, 2 colors'],
    [basicProfile(2, 2, 1, 0), 'Artwork, Low, Fixed colors'],
    [basicProfile(0, 0, 1, 5), 'Photograph, High, 5 colors'],
  ];
  assert.deepEqual(recorded.map(([profile]) => native.basicProfileLabel(profile)), recorded.map(([, label]) => label));
  const recent = new native.RecentBasicProfiles();
  for (const [profile] of recorded) assert.equal(recent.recordCompletion(profile, 0, true, false), true);
  const entries = rows => rows.map(([profile, label]) => ({ label, profile }));
  const newestFirst = recorded.slice(1).reverse();
  assert.deepEqual(recent.entries(), entries(newestFirst), 'the sixth completion evicts the oldest entry');
  assert.deepEqual(recent.visibleEntries(), entries(newestFirst.slice(0, 4)));

  // A duplicate label promotes the older complete profile and discards the incoming one.
  const duplicate = basicProfile(0, 1, 1, 8, { smoothness: 3, palette: [0, 0, 0, 1, 1, 1, 1, 1] });
  const promoted = entries([newestFirst.at(-1), ...newestFirst.slice(0, -1)]);
  assert.equal(recent.recordCompletion(duplicate, 0, true, false), true);
  assert.deepEqual(recent.entries(), promoted);
  assert.equal(recent.recordCompletion(duplicate, 0, true, false), true, 'an already-front duplicate is eligible');
  assert.deepEqual(recent.entries(), promoted);

  const fresh = basicProfile(2, 1, 2, 0);
  for (const [status, didWork, batch] of [[1, true, false], [2, true, false], [2 ** 32 - 1, true, false],
    [0, false, false], [0, true, true]]) {
    assert.equal(recent.recordCompletion(fresh, status, didWork, batch), false, `${status} ${didWork} ${batch}`);
  }
  // A Node-API u32 parameter would wrap 2**32 to the eligible status 0.
  for (const status of [-1, 0.5, 2 ** 32, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.throws(() => recent.recordCompletion(fresh, status, true, false), /unsigned 32-bit integer/);
  }
  assert.throws(() => recent.recordCompletion(fresh, '0', true, false));
  assert.deepEqual(recent.entries(), promoted);
  recent.clear();
  assert.deepEqual(recent.entries(), []);
  assert.deepEqual(recent.visibleEntries(), []);
});

test('Node recent basic persistence stores the projected list newest first', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-node-recent-'));
  try {
    const older = basicProfile(2, 0, 0, 2);
    const newer = basicProfile(0, 1, 1, 8, { palette: [0, 0.5, 1, 1, 1, 1, 1, 1], background: [0.25, 0.5, 0.75, 0.5] });
    const recent = new native.RecentBasicProfiles();
    recent.recordCompletion(older, 0, true, false);
    recent.recordCompletion(newer, 0, true, false);
    assert.deepEqual(recent.entries()[0].profile, newer, 'memory keeps palette colors and transparency');
    const stored = recent.storedProfiles();
    assert.deepEqual(stored.map(profile => native.basicProfileLabel(profile)),
      ['Photograph, Medium, 8 colors', 'Artwork, High, Two colors']);
    assert.deepEqual(stored.map(profile => [profile.settings.palette, profile.settings.background[3]]), [[[], 1], [[], 1]]);

    // Rebuilding the projection is stable, so saved files round-trip exactly.
    const rebuilt = new native.RecentBasicProfiles(stored);
    assert.deepEqual(rebuilt.entries().map(entry => entry.label), recent.entries().map(entry => entry.label));
    assert.deepEqual(rebuilt.storedProfiles(), stored);
    const file = join(directory, 'recent.json');
    recent.save(file);
    assert.deepEqual(JSON.parse(readFileSync(file, 'utf8')), { recentBasicSettings: stored });
    const loaded = new native.RecentBasicProfiles();
    loaded.load(file);
    assert.deepEqual(loaded.entries(), rebuilt.entries());

    // A failed read keeps the current list; a missing file is an empty collection.
    writeFileSync(join(directory, 'invalid.json'), 'not json');
    assert.throws(() => loaded.load(join(directory, 'invalid.json')));
    assert.deepEqual(loaded.entries(), rebuilt.entries());
    loaded.load(join(directory, 'missing.json'));
    assert.deepEqual(loaded.entries(), []);
    assert.throws(() => new native.RecentBasicProfiles({ recentBasicSettings: stored }));
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('Node recent basic replay forwards exact palette bounds, cancellation and cached fitting', () => {
  const scene = fixture('bitmap-scenes-reference.ndjson.gz').find(row => row.id === 'opaque-four-plugins');
  const load = () => {
    const document = new native.CoreSession().createDocument();
    document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
    return document;
  };
  // A real automatic run supplies classified basic controls for the replayed profiles.
  const automatic = load().runAutomatic({ extension: '.bmp' });
  const replay = paletteColorCount => ({ settings: { ...automatic.settings, paletteMode: 1 }, colorCount: 2,
    basicColorCount: 0, paletteColorCount, recommendedBackgroundMode: automatic.recommendedBackgroundMode });
  const runtime = { backup: [], customized: false };
  const exact = replay(3);
  assert.deepEqual(native.applyBasicProfile({ ...exact, settings: { ...exact.settings, advanced: true } }), exact);

  // Zero reaches the palette task unchanged; only advanced replay substitutes 2 through 12.
  const document = load();
  assert.throws(() => document.runBasicProfile(replay(0), runtime), /lower must be an integer from 2 through 24/);
  const advanced = document.runAdvancedProfile(replay(0), runtime);
  assert.equal(advanced.profile.settings.advanced, true);
  assert.equal(advanced.didWork.palette, true);
  assert.equal(document.runBasicProfile({ ...exact, settings: { ...exact.settings, paletteMode: 3 } }, runtime), null);

  const replayed = load();
  assert.throws(() => replayed.runBasicProfile(exact, runtime, { callback: () => false }), /cancelled/);
  assert.equal(replayed.snapshot().cancelled, true);
  const result = replayed.runBasicProfile(exact, runtime);
  assert.equal(result.profile.settings.advanced, false);
  assert.equal(result.profile.paletteColorCount, 3);
  assert.equal(result.didWork.fitting, true);
  assert.equal(result.state.fittingComplete, true);
  const cached = replayed.runBasicProfile(exact, runtime);
  assert.equal(cached.didWork.fitting, false);

  // JS only transports the completion; Rust decides that cached fitting does not record.
  const recent = new native.RecentBasicProfiles();
  assert.equal(recent.recordCompletion(cached.profile, cached.state.status, cached.didWork.fitting, false), false);
  assert.equal(recent.recordCompletion(result.profile, result.state.status, result.didWork.fitting, false), true);
  assert.deepEqual(recent.entries(), [{ label: native.basicProfileLabel(result.profile), profile: result.profile }]);
  assert.match(recent.entries()[0].label, /, 3 colors$/);
});

test('Node automatic, editing and batch results carry the Rust fitting completion', () => {
  const scene = fixture('bitmap-scenes-reference.ndjson.gz').find(row => row.id === 'opaque-four-plugins');
  const load = () => {
    const document = new native.CoreSession().createDocument();
    document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
    return document;
  };
  const document = load();
  const automatic = document.runAutomatic({ extension: '.bmp' });
  assert.deepEqual(automatic.fitting, { status: 0, didWork: true });
  const selected = { settings: automatic.settings, colorCount: 2, basicColorCount: 0,
    paletteColorCount: automatic.paletteCount ?? 0, recommendedBackgroundMode: automatic.recommendedBackgroundMode };
  const recent = new native.RecentBasicProfiles();
  const record = ({ status, didWork }, batch = false) => recent.recordCompletion(selected, status, didWork, batch);
  assert.equal(record(automatic.fitting), true);
  const recorded = [{ label: native.basicProfileLabel(selected), profile: selected }];
  assert.deepEqual(recent.entries(), recorded);

  // A committed edit refits. A cancelled finish yields no completion; its retry does the work.
  document.beginEditing();
  const initial = document.editing();
  document.edit({ tool: 'fill', x: 0, y: 0,
    color: Array.from(initial.bgra.subarray(0, 3), channel => channel < 128 ? 1 : 0).concat(1) });
  assert.throws(() => document.finishEditing(automatic.settings, { callback: () => false }), /cancelled/);
  const finished = document.finishEditing(automatic.settings);
  assert.deepEqual(finished.fittingCompletion, { status: 0, didWork: true });
  assert.equal(finished.fittingComplete, true);
  assert.equal(document.snapshot({ fitting: true }).fitting.complete, true, 'explicit capture keeps fitted geometry');
  assert.equal(record(finished.fittingCompletion), true);

  // Finishing unchanged edits reuses the cached fitting, which Rust does not record.
  const { fittingCompletion, ...cached } = document.finishEditing(automatic.settings);
  assert.deepEqual(fittingCompletion, { status: 0, didWork: false });
  assert.deepEqual(cached, document.snapshot({}));
  assert.equal(record(fittingCompletion), false);

  // Batch processing that refitted is still never recorded.
  const batchImage = load().runBatchImage({ extension: '.bmp', advanced: false, category: 3, quality: 3,
    paletteMode: 3, colorCount: 0, backgroundMode: 3, palette: [] });
  assert.deepEqual(batchImage.fitting, { status: 0, didWork: true });
  assert.equal(record(batchImage.fitting, true), false);
  assert.deepEqual(recent.entries(), recorded);
});

// Two flat regions keep engine work deterministic while exceeding the old
// one-million-value JSON limit in both a raster and the pixel label plane.
function largeTwoRegionBmp() {
  const width = 1024, height = 1024, stride = width * 3;
  const bytes = Buffer.alloc(54 + stride * height);
  bytes.write('BM');
  bytes.writeUInt32LE(bytes.length, 2);
  bytes.writeUInt32LE(54, 10);
  bytes.writeUInt32LE(40, 14);
  bytes.writeInt32LE(width, 18);
  bytes.writeInt32LE(height, 22);
  bytes.writeUInt16LE(1, 26);
  bytes.writeUInt16LE(24, 28);
  bytes.writeUInt32LE(stride * height, 34);
  for (let y = 0; y < height; y++) {
    bytes.fill(255, 54 + y * stride + stride / 2, 54 + (y + 1) * stride);
  }
  return bytes;
}

function assertSummary(state) {
  for (const key of ['original', 'source', 'working', 'segmentation', 'contours', 'smoothing', 'fitting']) {
    assert.equal(Object.hasOwn(state, key), false, `${key} must require explicit capture`);
  }
}

test('Node large typed snapshots preserve full planes, profile cache and caller ownership', () => {
  const session = new native.CoreSession();
  const document = session.createDocument();
  const source = largeTwoRegionBmp();
  document.setSource(source, 'two-regions.bmp');
  const before = document.snapshot({ rasters: ['original'], segmentation: 'full', fitting: true });
  assert.equal(Buffer.isBuffer(before.original.bgra), true);
  assert.equal(before.original.bgra.length, 1024 * 1024 * 4);
  assert.equal(before.segmentation, null);
  assert.equal(before.fitting, null);
  assert.equal(Object.hasOwn(before, 'working'), false);
  const profile = native.defaultAdvancedProfile();
  Object.assign(profile.settings, { paletteMode: 2, antiAlias: false, smoothingAntiAlias: false, backgroundMode: 0 });
  const runtime = { backup: [], customized: false };
  const stages = new Set();
  const processed = document.runAdvancedProfile(profile, runtime, { callback(event, checkpoint) {
    assertSummary(checkpoint);
    stages.add(event.stage);
    return true;
  } });
  assertSummary(processed.state);
  assert.equal(processed.state.fittingComplete, true);
  assert.deepEqual([...stages], ['segmentation', 'smoothing', 'fitting']);
  const cached = document.runAdvancedProfile(profile, runtime);
  assert.deepEqual(cached.didWork, { segmentation: false, smoothing: false, fitting: false });
  assert.equal(document.runTask('fitting', processed.profile.settings).didWork, false);
  assert.equal(document.snapshot({}).randomState, session.randomState());

  const captured = document.snapshot();
  const map = captured.segmentation;
  assert.ok(map.indices instanceof Int32Array);
  assert.ok(map.counts instanceof Int32Array);
  assert.ok(map.colorIndices instanceof Int32Array);
  assert.ok(map.palette instanceof Float32Array);
  assert.ok(captured.smoothing.referencePositions instanceof Float32Array);
  assert.ok(captured.smoothing.penaltyCounts instanceof Uint8Array);
  assert.ok(captured.fitting.cubicAvailability instanceof Uint8Array);
  assert.equal(map.indices.length, 1024 * 1024);
  assert.equal(map.counts.reduce((sum, count) => sum + count, 0), 1024 * 1024);
  assert.notEqual(map.indices[0], map.indices[1023]);
  assert.deepEqual(Array.from(captured.original.bgra.subarray(0, 4)), [0, 0, 0, 255]);
  assert.deepEqual(Array.from(captured.original.bgra.subarray(4092, 4096)), [255, 255, 255, 255]);
  const regions = document.snapshot({ segmentation: 'regions' });
  assert.equal(Object.hasOwn(regions.segmentation, 'indices'), false);
  assert.deepEqual(regions.segmentation.counts, map.counts);

  const expectedPixels = Buffer.from(captured.working.bgra);
  const expectedLabels = map.indices.slice();
  const expectedPalette = map.palette.slice();
  const expectedGeometry = structuredClone(captured.fitting.geometry);
  captured.working.bgra.fill(19);
  map.indices.fill(-1);
  map.palette.fill(0.123);
  captured.fitting.geometry.nodes[0].x = Number.NaN;
  const unchanged = document.snapshot({ rasters: ['working'], segmentation: 'full', fitting: true });
  assert.deepEqual(unchanged.working.bgra, expectedPixels);
  assert.deepEqual(unchanged.segmentation.indices, expectedLabels);
  assert.deepEqual(unchanged.segmentation.palette, expectedPalette);
  assert.deepEqual(unchanged.fitting.geometry, expectedGeometry);

  const editorCommand = document.beginEditing();
  assert.equal(Object.hasOwn(editorCommand, 'bgra'), false);
  const editor = document.editing();
  const editorPixels = Buffer.from(editor.bgra);
  editor.bgra.fill(7);
  assert.deepEqual(document.editing().bgra, editorPixels);
  document.gesture({ phase: 'select', tool: 'pencil' });
  document.gesture({ phase: 'color', color: [0, 0, 1, 1] });
  document.gesture({ phase: 'press', point: [4, 4], buttons: 1, button: 1 });
  const gesture = document.gesture({ phase: 'release', point: [4, 4], buttons: 0, button: 1 });
  assert.equal(Object.hasOwn(gesture, 'bgra'), false);
  assert.equal(gesture.undoCount, 1);
  const committed = document.commitEditing();
  assertSummary(committed);
  assert.equal(committed.fittingComplete, false);
  assert.deepEqual(unchanged.working.bgra, expectedPixels);
  document.reset();
  assert.equal(document.editing(), null);
  assert.equal(document.snapshot({ rasters: ['original'], fitting: true }).original, null);
  assert.deepEqual(unchanged.segmentation.indices, expectedLabels);
  assert.deepEqual(unchanged.fitting.geometry, expectedGeometry);
  assert.deepEqual(before.original.bgra.subarray(0, 4), Buffer.from([0, 0, 0, 255]));
});

test('Node selected smoothing checkpoint survives cancellation, retry and reset', () => {
  const scene = fixture('bitmap-scenes-reference.ndjson.gz').find(row => row.id === 'opaque-four-plugins');
  const session = new native.CoreSession();
  const document = session.createDocument();
  document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
  const { settings } = document.runAutomatic({ extension: '.bmp' });
  document.beginEditing();
  const initial = document.editing();
  document.edit({ tool: 'fill', x: 0, y: 0,
    color: Array.from(initial.bgra.subarray(0, 3), channel => channel < 128 ? 1 : 0).concat(1) });
  const committed = document.commitEditing();
  let checkpoint;
  const cancelled = document.runTask('smoothing', settings, {
    capture: { rasters: ['working'], segmentation: 'regions', contours: true, smoothing: true, fitting: true },
    callback(event, state) {
      if (event.phase !== 8 || event.substep !== -1) return true;
      checkpoint = state;
      return false;
    },
  });
  assert.ok(checkpoint, 'the first smoothing checkpoint must be delivered');
  assertSummary(cancelled.state);
  assert.equal(cancelled.state.status, 2);
  assert.equal(cancelled.state.cancelled, true);
  assert.equal(checkpoint.cancelled, false, 'checkpoint precedes callback cancellation');
  assert.equal(checkpoint.stageKeys.segmentationEdits, committed.stageKeys.segmentationEdits);
  assert.equal(checkpoint.fitting, null);
  assert.equal(Object.hasOwn(checkpoint, 'original'), false);
  assert.equal(Object.hasOwn(checkpoint.segmentation, 'indices'), false);
  assert.ok(checkpoint.smoothing.referencePositions instanceof Float32Array);
  assert.equal(checkpoint.randomState, session.randomState());
  const pixels = Buffer.from(checkpoint.working.bgra);
  const positions = checkpoint.smoothing.referencePositions.slice();
  const geometry = structuredClone(checkpoint.contours);
  const retry = document.finishEditing(settings);
  assert.deepEqual(retry.fittingCompletion, { status: 0, didWork: true });
  assert.equal(retry.cancelled, false);
  assert.deepEqual(document.finishEditing(settings).fittingCompletion, { status: 0, didWork: false });
  document.reset();
  assert.deepEqual(checkpoint.working.bgra, pixels);
  assert.deepEqual(checkpoint.smoothing.referencePositions, positions);
  assert.deepEqual(checkpoint.contours, geometry);
});

test('Node batch progress captures selected state and retains cancellation retry semantics', () => {
  const directory = mkdtempSync(join(tmpdir(), 'openvector-node-batch-capture-'));
  try {
    const scene = fixture('bitmap-scenes-reference.ndjson.gz').find(row => row.id === 'opaque-four-plugins');
    const source = join(directory, 'source.bmp');
    writeFileSync(source, Buffer.from(scene.sourceBytes, 'base64'));
    const batch = new native.CoreSession().createBatch(directory, directory);
    batch.addFiles([source]);
    batch.setExport(directory, 'svg');
    const settings = { advanced: false, category: 3, quality: 3, paletteMode: 3,
      colorCount: 0, backgroundMode: 3, palette: [] };
    let checkpoint;
    const cancelled = batch.run(settings, {}, undefined, {
      capture: { rasters: ['original'], segmentation: 'regions' },
      callback(event, state) {
        assertSummary(event.state);
        if (!state.original) return true;
        checkpoint = state;
        return false;
      },
    });
    assert.equal(cancelled.status, 'cancelled');
    assert.ok(Buffer.isBuffer(checkpoint.original.bgra));
    const pixels = Buffer.from(checkpoint.original.bgra);
    const retried = batch.run(settings, {}, undefined, { callback(event, state) {
      assertSummary(event.state);
      assertSummary(state);
      return true;
    } });
    assert.equal(retried.converted, 1);
    assert.equal(retried.files[0].status, 'written');
    assertSummary(batch.snapshot().document);
    batch.clear();
    assert.deepEqual(checkpoint.original.bgra, pixels);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});


test('Node saveVector never replaces a destination or publishes cancelled output', () => {
  const scene = fixture('bitmap-scenes-reference.ndjson.gz').find(row => row.id === 'opaque-four-plugins');
  const document = new native.CoreSession().createDocument();
  document.setSource(Buffer.from(scene.sourceBytes, 'base64'), scene.sourceFilename);
  document.runAutomatic({ extension: '.bmp' });
  const directory = mkdtempSync(join(tmpdir(), 'openvector-safe-save-'));
  try {
    const existing = join(directory, 'existing.svg');
    writeFileSync(existing, 'keep existing');
    const refused = document.saveVector(existing, {});
    assert.equal(refused.status, 4);
    assert.equal(readFileSync(existing, 'utf8'), 'keep existing');

    const cancelled = join(directory, 'cancelled.svg');
    assert.throws(() => document.saveVector(cancelled, {}, () => false), /cancelled/i);
    assert.equal(existsSync(cancelled), false);

    const raced = join(directory, 'raced.svg');
    let created = false;
    const result = document.saveVector(raced, {}, () => {
      if (!created) { writeFileSync(raced, 'won the race', { flag: 'wx' }); created = true; }
      return true;
    });
    assert.equal(result.status, 4);
    assert.equal(readFileSync(raced, 'utf8'), 'won the race');

    const fresh = join(directory, 'fresh.svg');
    assert.equal(document.saveVector(fresh, {}).status, 0);
    assert.match(readFileSync(fresh, 'utf8'), /<svg/);
    assert.deepEqual(readdirSync(directory).sort(), ['existing.svg', 'fresh.svg', 'raced.svg']);
    if (process.platform !== 'win32') {
      const ordinary = join(directory, 'ordinary');
      writeFileSync(ordinary, 'mode comparison');
      assert.equal(statSync(fresh).mode & 0o777, statSync(ordinary).mode & 0o777);
    }
    const other = join(directory, 'other');
    mkdirSync(other);
    const originalCwd = process.cwd();
    try {
      process.chdir(directory);
      let changed = false;
      const saved = document.saveVector('relative.svg', {}, () => {
        if (!changed) { process.chdir(other); changed = true; }
        return true;
      });
      assert.equal(saved.status, 0);
      assert.equal(existsSync(join(directory, 'relative.svg')), true);
      assert.equal(existsSync(join(other, 'relative.svg')), false);
    } finally { process.chdir(originalCwd); }
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
