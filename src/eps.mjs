import { readVectorOutput, walkVectorOutput, createScaledOutputFormatter, collectVectorText, validateVectorTextWriter } from './vector-output.mjs';

const weekdayNames = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const monthNames = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
function currentCreationDate() {
  const date = new Date(), twoDigits = value => String(value).padStart(2, '0');
  return `${weekdayNames[date.getDay()]} ${monthNames[date.getMonth()]} ${String(date.getDate()).padStart(2, ' ')} ` +
    `${twoDigits(date.getHours())}:${twoDigits(date.getMinutes())}:${twoDigits(date.getSeconds())} ${date.getFullYear()}`;
}

/** Export a completed fitted graph as EPS, preserving caller storage. */
export function exportEps(input) {
  return collectVectorText(serializeEps, input);
}

export function serializeEps(input, emit) {
  validateVectorTextWriter(emit, 'EPS');
  const options = readVectorOutput(input, 'EPS');
  const write = chunk => emit(chunk, chunk.endsWith('\n'));
  const { geometry, outputWidth, outputHeight, shapeMode, thinStroke } = options;
  const { compact = false, creationDate = currentCreationDate() } = input;
  if (typeof compact !== 'boolean' || typeof creationDate !== 'string' ||
      !/^[\x20-\x7e]{1,255}$/.test(creationDate)) {
    throw new Error('EPS compact must be true or false; creationDate must be 1..255 printable ASCII characters on one line.');
  }
  const commands = compact ? { move:'mt', curve:'ct', line:'lt', path:'np', close:'cp', color:'sc',
    strokeFill:'er' } : { move:'moveto', curve:'curveto', line:'lineto', path:'newpath', close:'closepath',
    color:'setrgbcolor', strokeFill:'gsave fill grestore 0.09375 setlinewidth stroke' };
  write('%!PS-Adobe-3.0 EPSF-3.0\n' + `%%CreationDate: ${creationDate}\n` +
    `%%LanguageLevel: 3\n%%BoundingBox: 0 0 ${outputWidth} ${outputHeight}\n%%EndComments\n`);
  if (compact) {
    for (const [alias, operator] of [['mt', 'moveto'], ['ct', 'curveto'], ['lt', 'lineto'], ['np', 'newpath'],
      ['cp', 'closepath'], ['sc', 'setrgbcolor'], ['er', 'gsave fill grestore 0.09375 setlinewidth stroke'], ['lw', 'setlinewidth']]) {
      write(`/${alias} { ${operator} } def\n`);
    }
  }
  const { point, component } = createScaledOutputFormatter(options);
  let pathStarted = false;
  const grouped = shapeMode === 2;
  const finishPath = color => {
    write(`${commands.close}\n${component(color[2])} ${component(color[1])} ${component(color[0])} ${commands.color}\n`);
    write(`${thinStroke ? commands.strokeFill : 'fill'}\n`);
  };
  walkVectorOutput(geometry, options, {
    acceptColor:color => color[3] > 50,
    // The common walker encloses every grouped shape in its color group.
    // EPS defers painting that whole path until endGroup.
    beginGroup() {},
    endGroup(color) { finishPath(color); pathStarted = false; },
    beginShape() {
      if (!pathStarted || !grouped) { write(`${commands.path}\n`); pathStarted = true; }
    },
    endShape(color) { if (!grouped) finishPath(color); },
    line(start, end, startKind) {
      if (startKind) write(`${point(start.x, start.y)} ${commands.move}\n`);
      write(`${point(end.x, end.y)} ${commands.line}\n`);
    },
    cubic(curve, startKind) {
      if (startKind) write(`${point(curve[0], curve[1])} ${commands.move}\n`);
      for (const index of [2, 4, 6]) write(`${point(curve[index], curve[index + 1])} `);
      write(`${commands.curve}\n`);
    },
  });
  write('%%EOF\n');
}
