import { readVectorOutput, walkVectorOutput, collectVectorText, validateVectorTextWriter } from './vector-output.mjs';
import { fixedDecimal, streamFixedDecimal } from './decimal-format.mjs';

const hexadecimal = byte => byte.toString(16).padStart(2, '0');
const rgbHex = color => hexadecimal(color[2]) + hexadecimal(color[1]) + hexadecimal(color[0]);

/** Export a completed fitContours graph without changing caller storage. */
export function exportSvg(input) {
  return collectVectorText(serializeSvg, input);
}

/** Emit initialized and traversed bytes before their progress checkpoints. */
export function serializeSvg(input, write) {
  validateVectorTextWriter(write, 'SVG');
  const options = readVectorOutput(input, 'SVG');
  const { geometry, width, height, outputWidth, outputHeight, thinStroke } = options;
  // Shared nodes and controls occur in adjacent paths. Cache only within this
  // export so legacy conversion runs once per distinct coordinate value.
  const coordinates = new Map();
  const coordinate = value => {
    if (!coordinates.has(value)) coordinates.set(value, streamFixedDecimal(value, 2));
    return coordinates.get(value);
  };
  const point = (x, y) => `${coordinate(x)} ${coordinate(y)}`;
  write('<?xml version="1.0" encoding="UTF-8" ?>\n' +
    '<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">\n', true);
  write(`<svg width="${outputWidth}pt" height="${outputHeight}pt" viewBox="0 0 ${width} ${height}" version="1.1" xmlns="http://www.w3.org/2000/svg">`);
  walkVectorOutput(geometry, options, {
    acceptColor:color => color[3] > 10,
    beginGroup(color) { write('\n', true); write(`<g id="#${rgbHex(color)}${hexadecimal(color[3])}">`); },
    endGroup() { write('\n', true); write('</g>'); },
    beginShape(color) {
      const rgb = rgbHex(color), alpha = color[3];
      write('\n', true);
      write(`<path fill="#${rgb}"`);
      if (thinStroke) write(` stroke="#${rgb}" stroke-width="0.09375"`);
      const opacity = alpha >= 251 ? '1.00' : fixedDecimal(alpha * (1 / 255), 2);
      write(` opacity="${opacity}" d="`);
    },
    endShape() { write(' Z" />'); },
    line(start, end, startKind) {
      if (startKind) write(` M ${point(start.x, start.y)}`);
      write(` L ${point(end.x, end.y)}`);
    },
    cubic(curve, startKind) {
      if (startKind) write(` M ${point(curve[0], curve[1])}`);
      write(' C');
      for (const control of [2, 4, 6]) write(` ${point(curve[control], curve[control + 1])}`);
    },
  });
  write('\n</svg>\n', true);
}
