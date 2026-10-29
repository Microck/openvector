import assert from 'node:assert/strict';
import { test } from 'node:test';
import reference from './fixtures/pipeline-prepared-reference.json' with { type:'json' };
import { CoreSession } from '../src/palette.mjs';
import { segment } from '../src/segmentation.mjs';
import { extractContours } from '../src/contours.mjs';
import { fitContours } from '../src/contour-fitting.mjs';
import { exportSvg } from '../src/svg.mjs';
import { exportEps } from '../src/eps.mjs';
import { exportPdf } from '../src/pdf.mjs';
import { exportAi } from '../src/ai.mjs';
import { exportDxf } from '../src/dxf.mjs';
import { exportEmf } from '../src/emf.mjs';

test('all public stages through all six vector formats match 15 original pipelines in one retained application session', () => {
  assert.equal(reference.cases.length, 15);
  const session = new CoreSession();
  for (const frame of reference.cases) {
    const { width, height, parameters } = frame, bgra = frame.preparedBgra;
    const input = { width, height, bgra, colorCount:frame.colorCount, bins:frame.bins, connected:frame.connected };
    const palette = frame.antiAlias ? session.generateWeighted(input) : session.generateUnweighted(input);
    assert.deepEqual({ palette:[...palette.palette], indices:[...palette.indices], weights:[...palette.weights],
      randomState:session.randomState }, frame.paletteStage, `${frame.id} palette`);
    const segmented = segment({ width, height, bgra, parameters });
    assert.deepEqual([...segmented.indices], frame.segmentationStage.indices, `${frame.id} segmentation`);
    assert.deepEqual([...segmented.palette], frame.segmentationStage.palette, `${frame.id} region colors`);
    const geometry = extractContours({ ...segmented, width, height, antiAlias:frame.antiAlias });
    assert.deepEqual(geometry, frame.contourStage, `${frame.id} contours`);
    assert.equal(session.randomState, frame.randomStates.contourExtraction);
    const smoothed = session.smoothContours({ width, height, bgra, ...segmented, geometry, parameters });
    assert.deepEqual(smoothed.geometry, frame.smoothingStage, `${frame.id} smoothing`);
    assert.deepEqual([...smoothed.penaltyCounts], frame.smoothingMetadata.penaltyCounts);
    assert.equal(session.randomState, frame.randomStates.smoothing);
    const fitted = fitContours({ geometry:smoothed.geometry, parameters });
    assert.deepEqual(fitted, frame.geometry, `${frame.id} fitting`);
    assert.equal(session.randomState, frame.randomStates.fitting);
    assert.equal(exportSvg({ geometry:fitted, width, height }), frame.svg, `${frame.id} SVG bytes`);
    assert.equal(exportEps({ geometry:fitted, width, height, creationDate:frame.epsCreationDate }), frame.eps, `${frame.id} EPS bytes`);
    assert.equal(Buffer.from(exportPdf({ geometry:fitted, width, height })).toString('latin1'), frame.pdf, `${frame.id} PDF bytes`);
    assert.equal(exportAi({ geometry:fitted, width, height }), frame.ai, `${frame.id} AI bytes`);
    assert.equal(exportDxf({ geometry:fitted, palette:segmented.palette, width, height }), frame.dxf, `${frame.id} DXF bytes`);
    assert.equal(Buffer.from(exportEmf({ geometry:fitted, width, height, referenceDevice:frame.emfReferenceDevice })).toString('latin1'), frame.emf, `${frame.id} EMF bytes`);
    assert.equal(session.randomState, frame.randomStates.fitting);
  }
});
