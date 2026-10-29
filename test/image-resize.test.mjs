import assert from 'node:assert/strict';
import { createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import { createGunzip } from 'node:zlib';
import test from 'node:test';
import { resizePreparedImage, fitResizeGeometry, reduceWorkingImage } from '../src/image-resize.mjs';
import { prepareImagePixels } from '../src/image-preparation.mjs';

async function* referenceRows(scope) {
  const rows = createInterface({ input:createReadStream(new URL(`./fixtures/image-resize-${scope}-reference.ndjson.gz`, import.meta.url))
    .pipe(createGunzip()), crlfDelay:Infinity });
  for await (const line of rows) yield JSON.parse(line);
}

test('Q8 sampling and triangle resizing match native bytes and preserve input ownership', async () => {
  let expectedCases, cases = 0;
  for await (const frame of referenceRows('kernel')) {
    if (frame.type === 'metadata') {
      assert.equal(frame.wholeNativeResponseRepeatIdentical, true);
      assert.equal(frame.originalInstructionsPatched, false);
      expectedCases = frame.cases; continue;
    }
    const bgra = Uint8Array.from(Buffer.from(frame.bgra, 'base64')), before = bgra.slice();
    const resized = resizePreparedImage({ ...frame, bgra });
    assert.deepEqual([resized.width, resized.height], frame.expectedDimensions, frame.id);
    assert.deepEqual(Buffer.from(resized.bgra), Buffer.from(frame.expected, 'base64'), frame.id);
    assert.deepEqual(bgra, before, frame.id); assert.notEqual(resized.bgra, bgra, frame.id);
    cases++;
  }
  assert.equal(cases, expectedCases);
});

test('native WxH geometry preserves asymmetric float32 height-ratio arithmetic', async () => {
  let expectedCases, cases = 0;
  for await (const frame of referenceRows('geometry')) {
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical, true); expectedCases = frame.cases; continue; }
    assert.deepEqual(fitResizeGeometry(frame), frame.expected, JSON.stringify(frame));
    cases++;
  }
  assert.equal(cases, expectedCases);
});

test('complete large-image preparation matches native working and retained cache pixels', async () => {
  let expectedCases, cases = 0;
  for await (const frame of referenceRows('working')) {
    if (frame.type === 'metadata') { assert.equal(frame.wholeNativeResponseRepeatIdentical, true); expectedCases = frame.cases; continue; }
    const bgra = Uint8Array.from(Buffer.from(frame.bgra, 'base64')), before = bgra.slice();
    const prepared = prepareImagePixels({ ...frame, bgra });
    assert.deepEqual([prepared.width, prepared.height], frame.preparedDimensions, frame.id);
    assert.deepEqual(Buffer.from(prepared.bgra), Buffer.from(frame.preparedBgra, 'base64'), frame.id);
    const retained = prepared.bgra.slice();
    // The decoded BGRA adapter requests matte for every populated source.
    // Its captured matte output is an assertion, not a runtime intermediate.
    if (prepared.width) assert.equal(frame.matte, 1);
    const working = reduceWorkingImage({ ...prepared, antiAlias:frame.antiAlias, matte:1 });
    assert.deepEqual([working.width, working.height], frame.expectedDimensions, frame.id);
    assert.deepEqual(Buffer.from(working.bgra), Buffer.from(frame.expected, 'base64'), frame.id);
    assert.deepEqual(bgra, before, frame.id); assert.deepEqual(prepared.bgra, retained, frame.id);
    assert.notEqual(working.bgra, prepared.bgra, frame.id);
    cases++;
  }
  assert.equal(cases, expectedCases);
});
