import { readVectorOutput, walkVectorOutput, createScaledOutputFormatter, collectVectorText, validateVectorTextWriter } from './vector-output.mjs';

/** Export the reference AI dialect with its native header and compound paths. */
export function exportAi(input) {
  return collectVectorText(serializeAi, input);
}

export function serializeAi(input, write) {
  validateVectorTextWriter(write, 'AI');
  const options = readVectorOutput(input, 'AI');
  const { geometry, width, height, thinStroke } = options;
  const { fixed, point, component } = createScaledOutputFormatter(options);
  // Empty metadata fields retain a trailing space. The numeric header sets
  // fixed formatting before any RGB operands, matching the native stream.
  write(`%!PS-Adobe-3.0
%%Creator: GNU libplot drawing library 4.1
%%For: (Unknown) (Unknown)
%%Title: (Untitled)
%%CreationDate:\x20
%%BoundingBox: 0 0 ${width} ${height}
%%HiResBoundingBox: 0.0000 0.0000 ${fixed(width, 2)} ${fixed(height, 2)}
%%DocumentProcessColors: Cyan Magenta Yellow Black
%%DocumentFonts:\x20
%%DocumentNeededResources: procset Adobe_level2_AI5 1.0 0
%%+ procset Adobe_typography_AI5 1.0 0
%%+ procset Adobe_Illustrator_AI6_vars Adobe_Illustrator_AI6
%%+ procset Adobe_Illustrator_AI5 1.0 0
%AI5_FileFormat 3
%AI3_ColorUsage: Color
%AI7_ImageSettings: 0
%AI3_DocumentPreview: None
%AI5_ArtSize: ${width} ${height}
%AI5_RulerUnits: 0
%AI5_ArtFlags: 1 0 0 1 0 0 1 1 0
%AI5_TargetResolution: 800
%AI5_NumLayers: 1
%AI5_OpenViewLayers: 7
%%PageOrigin:0 0
%%AI3_PaperRect:0 0${width} ${height}
%%AI3_Margin:0 0 0 0
%AI7_GridSettings: 72 8 72 8 1 0 0.8 0.8 0.8 0.9 0.9 0.9
%%EndComments
%%BeginProlog
%%IncludeResource: procset Adobe_level2_AI5 1.0 0
%%IncludeResource: procset Adobe_typography_AI5 1.0 0
%%IncludeResource: procset Adobe_Illustrator_AI6_vars Adobe_Illustrator_AI6
%%IncludeResource: procset Adobe_Illustrator_AI5 1.0 0
%%EndProlog
%%BeginSetup
Adobe_level2_AI5 /initialize get exec
Adobe_Illustrator_AI5_vars Adobe_Illustrator_AI5 Adobe_typography_AI5 /initialize get exec
Adobe_ColorImage_AI6 /initialize get exec
Adobe_Illustrator_AI5 /initialize get exec
%%EndSetup
%AI5_BeginLayer
1 1 1 1 0 0 0 79 128 255 Lb
(Layer 1) Ln
10.43 M
0.09375 w
[] 0.0000 d
`);
  const paint = thinStroke ? 'b\n' : 'f\n';
  const beginPath = (x, y, startKind) => {
    if (startKind === 2) write(paint);
    if (startKind) write(`${point(x, y)} m\n`, true);
  };
  walkVectorOutput(geometry, options, {
    acceptColor:color => color[3] > 50,
    beginGroup() { write('u\n'); },
    endGroup() { write('U\n'); },
    beginShape(color, hasHoles) {
      const rgb = `${component(color[2])} ${component(color[1])} ${component(color[0])}`;
      write(`${rgb} Xa\n`, true);
      if (thinStroke) write(`${rgb} XA\n`, true);
      if (hasHoles) write('u\n*u\n');
    },
    endShape(color, hasHoles) { write(paint); if (hasHoles) write('*U\nU\n'); },
    line(start, end, startKind) {
      beginPath(start.x, start.y, startKind);
      write(`${point(end.x, end.y)} L\n`, true);
    },
    cubic(curve, startKind) {
      beginPath(curve[0], curve[1], startKind);
      for (const index of [2, 4, 6]) write(`${point(curve[index], curve[index + 1])} `);
      write(' C\n', true);
    },
  });
  write(`LB
%AI5_EndLayer--
%%PageTrailer
gsave annotatepage grestore showpage
%%Trailer
Adobe_Illustrator_AI5 /terminate get exec
Adobe_ColorImage_AI6 /terminate get exec
Adobe_typography_AI5 /terminate get exec
Adobe_level2_AI5 /terminate get exec
%%EOF
`);
}
