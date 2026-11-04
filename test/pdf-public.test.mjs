import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import reference from './fixtures/pdf-cli-reference.json' with { type: 'json' };

test('public PDF identifies OpenVector and preserves exact reference bytes and xref', () => {
  const cli = fileURLToPath(new URL('../bin/openvector.mjs', import.meta.url));
  for (const fixture of reference.cases) {
    const run = spawnSync(process.execPath, [cli, 'export-pdf', '-'], { input: JSON.stringify(fixture.input) });
    assert.equal(run.status, 0, run.stderr.toString());
    const expected = fixture.expected.replace('/Producer(VectorMagic.com)', '/Producer(OpenVector)     ');
    assert.notEqual(expected, fixture.expected);
    assert.deepEqual(run.stdout, Buffer.from(expected, 'latin1'));
    const pdf = run.stdout.toString('latin1');
    const match = /startxref\r\n(\d+)\r\n%%EOF/.exec(pdf);
    assert.ok(match);
    assert.equal(pdf.slice(Number(match[1]), Number(match[1]) + 4), 'xref');
  }
});
