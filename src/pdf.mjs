import { readVectorOutput, walkVectorOutput, createScaledOutputFormatter, collectVectorText, validateVectorTextWriter } from './vector-output.mjs';

/** Export an owned byte array; the binary header is not UTF-8 text. */
export function exportPdf(input) {
  const pdf = collectVectorText(serializePdf, input);
  const bytes = new Uint8Array(pdf.length);
  for (let index = 0; index < pdf.length; index++) bytes[index] = pdf.charCodeAt(index);
  return bytes;
}

/** Emit raw text-mode bytes, including the single-byte binary header. */
export function serializePdf(input, emit) {
  validateVectorTextWriter(emit, 'PDF');
  const options = readVectorOutput(input, 'PDF');
  const { geometry, width, height, shapeMode, thinStroke } = options;
  const { point, component } = createScaledOutputFormatter(options);
  // Count emitted bytes rather than retaining a second complete document.
  // A UTF-8 count would corrupt the offsets after the binary header.
  let position = 0;
  const write = chunk => {
    position += chunk.length;
    for (const character of chunk) if (character === '\n') position++;
    emit(chunk, chunk.endsWith('\n'));
  };
  write('%PDF-1.4\n%\xc7\xec\x8f\xa2\n');
  const offsets = new Array(9).fill(-1);
  const beginObject = number => { offsets[number] = position; write(`${number} 0 obj\n`); };
  beginObject(5);
  write('<</Length 6 0 R>>\nstream\n');
  const contentStart = position;
  write('0.09375 w\n');
  const paint = thinStroke ? 'b\n' : 'f\n';
  const grouped = shapeMode === 2, usedAlpha = new Uint8Array(256);
  let currentAlpha = 255;
  walkVectorOutput(geometry, options, {
    acceptColor:color => color[3] > 10,
    beginGroup() {},
    endGroup() { write(paint); },
    beginShape(color) {
      const rgb = `${component(color[2])} ${component(color[1])} ${component(color[0])}`;
      write(`${rgb} RG\n${rgb} rg\n`);
      const alpha = color[3];
      if (alpha !== currentAlpha) {
        write(`/Vmt${alpha} gs\n`);
        currentAlpha = alpha; usedAlpha[alpha] = 1;
      }
    },
    endShape() { if (!grouped) write(paint); },
    line(start, end, startKind) {
      if (startKind) write(`${point(start.x, start.y)} m\n`);
      write(`${point(end.x, end.y)} l\n`);
    },
    cubic(curve, startKind) {
      if (startKind) write(`${point(curve[0], curve[1])} m\n`);
      for (const index of [2, 4, 6]) write(`${point(curve[index], curve[index + 1])} `);
      write('c\n');
    },
  });
  // The native length includes the CR of this extra terminating CRLF.
  const contentLength = position - contentStart + 1;
  write('\nendstream\nendobj\n');
  beginObject(6);
  write(`${contentLength}\nendobj\n`);
  beginObject(4);
  // The reference keeps the source page box even for scaled output paths.
  write(`<</Type/Page/MediaBox [0 0 ${width} ${height}]\n` +
    '/Parent 3 0 R\n/Resources<</ProcSet[/PDF]\n/ExtGState 8 0 R\n>>\n/Contents 5 0 R\n>>\nendobj\n');
  beginObject(3);
  write('<< /Type /Pages /Kids [\n4 0 R\n] /Count 1\n>>\nendobj\n');
  beginObject(1);
  write('<</Type /Catalog /Pages 3 0 R\n>>\nendobj\n');
  beginObject(7);
  write('<</Type/ExtGState\n/OPM 1>>endobj\n');
  beginObject(8);
  write('<</R7\n7 0 R\n');
  for (let alpha = 0; alpha < usedAlpha.length; alpha++) {
    if (usedAlpha[alpha] !== 0) {
      const opacity = component(alpha);
      write(`/Vmt${alpha} <</CA ${opacity}/ca ${opacity}>>\n`);
    }
  }
  write('>>endobj\n');
  beginObject(2);
  // Match the native writer without shifting the retained reference's offsets.
  write('<</Producer(OpenVector)     \n>>endobj\n');
  const xref = position;
  write('xref\n0 9\n0000000000 65535 f\n');
  for (let number = 1; number < offsets.length; number++) write(`${String(offsets[number]).padStart(10, '0')} 00000 n\n`);
  write('trailer\n<< /Size 10 /Root 1 0 R /Info 2 0 R\n>>\nstartxref\n' + `${xref}\n%%EOF\n`);
}
