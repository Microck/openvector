import { partitionContourInterval, fitContourInterval } from './fitting.mjs';

function markedSample(nodes, contour, sample, direction, skippedKind) {
  const original = contour[sample % contour.length];
  let current = contour.length + sample;
  do {
    current += direction;
    if (nodes[contour[current % contour.length]].kind !== skippedKind) break;
  } while (contour[current % contour.length] !== original);
  return current % contour.length;
}

function selectIntervals(nodes, regions, parameters, checkpoint) {
  let cubicStorageSlots = 0;
  for (const [index, region] of regions.entries()) {
    if (index % 4 === 0) checkpoint(1, Math.floor(index * 50 / regions.length + 0.5));
    const contour = region.contour;
    for (let sample = 0; sample < contour.length; sample++) {
      if (nodes[contour[sample]].kind !== 0) continue;
      const start = markedSample(nodes, contour, sample, -1, 0);
      let end = markedSample(nodes, contour, sample, 1, 0);
      while (end <= start) end += contour.length;
      const { records } = partitionContourInterval({ nodes, contour, start, end,
        initialThreshold:parameters['BezierFitter::stat_thresh_initial'],
        finalThreshold:parameters['BezierFitter::stat_thresh_final'],
        maxIterations:parameters['BezierFitter::max_iterations'], fraction:0.5 });
      let current = start;
      for (const [index, record] of records.entries()) {
        const next = index + 1 < records.length ? records[index + 1].start : end - start + 1;
        nodes[contour[current++ % contour.length]].kind = 5;
        for (let interior = record.start + 1; interior < next; interior++) {
          const node = nodes[contour[current++ % contour.length]];
          node.kind = 1; node.cubic = cubicStorageSlots;
        }
        // The endpoint sentinel reserves a slot as well. It may remain
        // uninitialized and must never be serialized as an actual cubic.
        cubicStorageSlots++;
      }
      nodes[contour[start % contour.length]].kind = 4;
      nodes[contour[end % contour.length]].kind = 4;
    }
  }
  return cubicStorageSlots;
}

function fitIntervals(nodes, regions, curves) {
  for (const region of regions) {
    const contour = region.contour;
    for (let sample = 0; sample < contour.length; sample++) {
      const node = nodes[contour[sample]];
      if (node.kind !== 1) continue;
      const start = markedSample(nodes, contour, sample, -1, 1);
      let end = markedSample(nodes, contour, sample, 1, 1);
      while (end <= start) end += contour.length;
      curves[node.cubic] = fitContourInterval({ nodes, contour, start, end, markInterior:true }).curve;
    }
  }
}

function fittedEdges(nodes, region, curves) {
  const contour = region.contour, cycle = contour.length, edges = [];
  let start = cycle;
  while (nodes[contour[start % cycle]].kind === 2) start--;
  start %= cycle;
  for (let current = start; current < start + cycle; current++) {
    const sample = current % cycle, previous = (current + cycle - 1) % cycle, next = (current + 1) % cycle;
    const node = nodes[contour[sample]], neighbor = region.boundary[sample][1];
    if (node.kind !== 2 && nodes[contour[next]].kind !== 2) {
      edges.push([sample, 0, neighbor, contour[sample], contour[next], sample, next]);
    }
    if (node.kind === 2) {
      const index = node.cubic, first = nodes[contour[previous]], curve = curves[index];
      while (nodes[contour[current % cycle]].kind === 2) current++;
      const end = current % cycle;
      const reversed = Math.abs(curve[0][0] - first.x) > 9.999999974752427e-7 ||
        Math.abs(curve[0][1] - first.y) > 9.999999974752427e-7;
      edges.push([index, reversed ? 2 : 1, neighbor, contour[previous], contour[end], previous, end]);
      current--;
    }
  }
  return edges;
}

function validateParameters(parameters) {
  const initial = parameters?.['BezierFitter::stat_thresh_initial'];
  const final = parameters?.['BezierFitter::stat_thresh_final'];
  const iterations = parameters?.['BezierFitter::max_iterations'];
  if (!Number.isFinite(initial) || initial <= 0 || !Number.isFinite(final) || final <= 0 ||
      !Number.isFinite(final / initial) || final / initial <= 0 ||
      !Number.isInteger(iterations) || iterations < 0 || iterations > 0x7fffffff) {
    throw new Error('Fitting needs positive finite thresholds with a finite ratio and a nonnegative int32 iteration count.');
  }
}

function validateGeometry(geometry) {
  if (!Array.isArray(geometry?.nodes) || geometry.nodes.length < 1 ||
      !Array.isArray(geometry.regions) || geometry.regions.length < 1) {
    throw new Error('Fitting needs shared nodes and completed region contours.');
  }
  for (const node of geometry.nodes) {
    if (!node || !Number.isFinite(node.x) || !Number.isFinite(node.y) ||
        !Number.isInteger(node.kind) || node.kind < 0 || node.kind > 255) {
      throw new Error('Fitting needs finite node coordinates and byte kinds.');
    }
  }
  for (const region of geometry.regions) {
    if (!region || !Array.isArray(region.bgra) || region.bgra.length !== 4 ||
        region.bgra.some(value => !Number.isInteger(value) || value < 0 || value > 255) ||
        !Number.isInteger(region.pixelCount) || region.pixelCount < 1 || region.pixelCount > 0x7fffffff ||
        !Number.isInteger(region.colorIndex) || region.colorIndex < 0 || region.colorIndex > 0x7fffffff ||
        !Number.isInteger(region.parent) || region.parent < -1 || region.parent >= geometry.regions.length ||
        !Array.isArray(region.contour) || region.contour.length < 3 ||
        !Array.isArray(region.boundary) || region.boundary.length !== region.contour.length) {
      throw new Error('Fitting needs region colors, counts, parents and matching contour boundary records.');
    }
    for (const [sample, node] of region.contour.entries()) {
      const boundary = region.boundary[sample];
      if (!Number.isInteger(node) || node < 0 || node >= geometry.nodes.length ||
          !Array.isArray(boundary) || boundary.length !== 3 ||
          !Number.isInteger(boundary[0]) || boundary[0] < 0 || boundary[0] > 255 ||
          !Number.isInteger(boundary[1]) || boundary[1] < -1 || boundary[1] >= geometry.regions.length ||
          !Number.isInteger(boundary[2]) || boundary[2] < 1 || boundary[2] > 0x7fffffff) {
        throw new Error('Fitting needs existing contour identifiers and valid boundary flags, neighbors and sample counts.');
      }
    }
  }
}

// Native float32 range endpoints, promoted to double before interpolation.
// Substep 1 spans counters 0..50; every other fitting range is constant.
const fittingRanges = {
  '-1':[0.004614000208675861, 0.004614000208675861],
  0:[0.005648000165820122, 0.005648000165820122],
  1:[0.00572500005364418, 0.9383379817008972],
  2:[0.9716119766235352, 0.9716119766235352],
  3:[0.9902650117874146, 0.9902650117874146],
  '-2':[0.994268000125885, 0.994268000125885],
};

/** Own one live fitting graph across runs. Observer exceptions stop the
 * current run without rolling back nodes, retained edges or cubic storage. */
export class ContourFitter {
  #nodes;
  #regions;
  #curves = [];

  constructor(geometry) {
    this.setRegions(geometry);
  }

  /** Region reconstruction replaces their identities and edge storage. The
   * cubic allocation survives until fitting finishes keeper selection. */
  setRegions(geometry) {
    validateGeometry(geometry);
    this.#nodes = geometry.nodes.map(({x,y,kind}) => ({x,y,cubic:-1,kind}));
    this.#regions = structuredClone(geometry.regions);
    for (const region of this.#regions) region.edges = [];
  }

  /** Smoothing rebuilds nodes/contours in the existing region records. Their
   * old edges survive until fitting rebuilds them, just like cubic storage. */
  setContours(geometry) {
    validateGeometry(geometry);
    if (geometry.regions.length !== this.#regions.length)
      throw new Error('Contour replacement must preserve region identities. Use setRegions after region reconstruction.');
    const regions = structuredClone(geometry.regions);
    for (const [index, region] of regions.entries()) region.edges = this.#regions[index].edges;
    this.#regions = regions;
    this.#nodes = geometry.nodes.map(({x,y,kind}) => ({x,y,cubic:-1,kind}));
  }

  get cubicAvailability() {
    return Array.from(this.#curves,curve => Number(curve !== undefined));
  }

  snapshot() {
    const cubics = [],cubicAvailability = Array(this.#curves.length).fill(0);
    // Reserved endpoint slots have never received coordinates. Preserve that
    // absence even when a previous run's edges still refer to the allocation.
    for (const [index, curve] of this.#curves.entries()) if (curve !== undefined) {
      cubicAvailability[index] = 1;cubics.push({index,points:curve.flat()});
    }
    const regions = this.#regions.map(({bgra,pixelCount,colorIndex,parent,contour,edges}) =>
      ({bgra,pixelCount,colorIndex,parent,contour,edges}));
    return structuredClone({cubicStorageSlots:this.#curves.length,cubicAvailability,cubics,nodes:this.#nodes,regions});
  }

  run(parameters,onProgress) {
    validateParameters(parameters);
    if (onProgress !== undefined && typeof onProgress !== 'function') throw new TypeError('Fitting onProgress must be a function.');
    let previousSubstep,previousCounter;
    const checkpoint = (substep,counter = 0) => {
      if (substep === previousSubstep && counter === previousCounter) return;
      previousSubstep = substep;previousCounter = counter;
      const [start,end] = fittingRanges[substep];
      const fraction = counter <= 0 ? start : counter >= 50 ? end : (end-start)*(counter/50)+start;
      onProgress?.({phase:10,substep,counter,fraction,percent:Math.floor(fraction*100+0.5)});
    };
    checkpoint(-1);
    // Reset kinds without erasing old cubic identifiers, coordinates or
    // edges. Their native lifetimes end at separate producers below.
    for (const node of this.#nodes)
      node.kind = node.kind === 1 || node.kind === 2 || node.kind === 5 ? 0 : node.kind === 4 ? 3 : node.kind;
    checkpoint(0);
    const slots = selectIntervals(this.#nodes,this.#regions,parameters,checkpoint);
    this.#curves = new Array(slots);
    checkpoint(2);
    fitIntervals(this.#nodes,this.#regions,this.#curves);
    checkpoint(3);
    for (const region of this.#regions) region.edges = fittedEdges(this.#nodes,region,this.#curves);
    checkpoint(-2);
    const {cubicStorageSlots,cubics,nodes,regions} = this.snapshot(),referenced = new Set();
    for (const region of regions) for (const edge of region.edges) if (edge[1] !== 0) referenced.add(edge[0]);
    return {cubicStorageSlots,cubics:cubics.filter(curve => referenced.has(curve.index)),nodes,regions};
  }
}

/** Fit a completed smoothing graph without changing caller storage. */
export function fitContours(input) {
  const {geometry,parameters,onProgress} = input ?? {};
  validateParameters(parameters);
  return new ContourFitter(geometry).run(parameters,onProgress);
}
