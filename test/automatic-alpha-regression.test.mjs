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
    assert.match(svg, /fill="#ff0000"[^>]*opacity="0\\.5/i);
  } finally { rmSync(dir, {recursive:true, force:true}); }
});
