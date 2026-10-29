// Original 0x4adfb0 reads six fields of the 21-double feature vector. Negated
// comparisons preserve its x87 unordered branches when an intermediate is NaN.
export function classifyCornerFeatures(features) {
  if (!(features[6] < 0.611194)) {
    if (!(features[15] < 0.5)) {
      if (!(features[16] < 0.097537)) return false;
      return !(features[6] < 1.42496) || features[2] < 0.122841 ||
        features[0] < 0.445642 || features[8] < 0.0046405;
    }
    return !(features[6] < 1.72002);
  }
  if (features[6] < 0.282594 || !(features[0] < 0.322933)) return false;
  return features[0] < 0.0286571 || !(features[6] < 0.483023) || features[2] < 0.00209524;
}

function readCornerGeometry(input) {
  const geometry = input?.geometry;
  if (!Array.isArray(geometry?.nodes) || geometry.nodes.length < 1 ||
      !Array.isArray(geometry.regions) || geometry.regions.length < 1) {
    throw new Error('Corner puncturing needs shared nodes and region contours.');
  }
  for (const node of geometry.nodes) {
    if (!node || !Number.isFinite(node.x) || !Number.isFinite(node.y) ||
        !Number.isInteger(node.kind) || node.kind < 0 || node.kind > 255 ||
        !Number.isInteger(node.flags) || node.flags < 0 || node.flags > 255) {
      throw new Error('Corner puncturing needs finite coordinates, byte kinds and byte flags.');
    }
  }
  for (const region of geometry.regions) {
    if ((!Array.isArray(region?.contour) && !(region?.contour instanceof Int32Array)) || region.contour.length < 3 ||
        !Array.isArray(region.boundary) || region.boundary.length !== region.contour.length) {
      throw new Error('Corner puncturing needs matching contour nodes and boundary records.');
    }
    for (const [sample, node] of region.contour.entries()) {
      const boundary = region.boundary[sample];
      if (!Number.isInteger(node) || node < 0 || node >= geometry.nodes.length ||
          !Array.isArray(boundary) || boundary.length !== 3 || !Number.isInteger(boundary[0]) || boundary[0] < 0 || boundary[0] > 255) {
        throw new Error('Corner puncturing needs existing node identifiers and byte boundary flags.');
      }
    }
  }
  return structuredClone(geometry);
}

function cornerTurns(nodes, contour) {
  const directions = [];
  for (let edge = 0; edge < contour.length; edge++) {
    const first = nodes[contour[edge]];
    const last = nodes[contour[(edge + 1) % contour.length]];
    let x = last.x - first.x, y = last.y - first.y;
    const length = Math.sqrt(y * y + (x * x + 0));
    // The native path retains a zero direction for collapsed edges.
    if (length > 0) {
      const inverse = 1 / length;
      x = inverse * x;
      y = inverse * y;
    }
    directions.push([x, y]);
  }
  const turns = new Float64Array(contour.length);
  for (let sample = 0; sample < contour.length; sample++) {
    const first = directions[(sample + contour.length - 1) % contour.length], last = directions[sample];
    const dot = last[1] * first[1] + (last[0] * first[0] + 0);
    turns[sample] = 2 - (dot + dot);
  }
  return turns;
}

/** Puncture corners in region order while retaining shared-node markers. */
export function punctureContourCorners(input) {
  const geometry = readCornerGeometry(input);
  const features = new Float64Array(21), turns = new Float64Array(5);
  for (const region of geometry.regions) {
    if (region.contour.length <= 6) continue;
    // Puncturing changes kinds/flags only. Positions stay fixed, so each
    // directed edge and turn can be evaluated once without changing arithmetic.
    const regionTurns = cornerTurns(geometry.nodes, region.contour);
    for (let sample = 0; sample < region.contour.length; sample++) {
      for (let turn = 0; turn < 5; turn++) turns[turn] = regionTurns[(sample + turn + region.contour.length - 2) % region.contour.length];
      let maximum = turns[0];
      for (let turn = 1; turn < 5; turn++) if (maximum < turns[turn]) maximum = turns[turn];
      const peak = turns[2] === maximum;
      if (peak) region.boundary[sample][0] |= 8;
      const leftIsWeaker = !(turns[1] > turns[3]);
      const weakOuter = turns[leftIsWeaker ? 0 : 4], weakInner = turns[leftIsWeaker ? 1 : 3];
      const strongOuter = turns[leftIsWeaker ? 4 : 0];
      let minimum = turns[0];
      if (turns[1] < minimum) minimum = turns[1];
      if (turns[3] < minimum) minimum = turns[3];
      if (turns[4] < minimum) minimum = turns[4];
      // Length and median fields are unused by the recovered fixed decision
      // tree. Evaluate its six inputs directly, preserving native sum order.
      features[0] = (turns[1] + turns[3]) + ((turns[0] + turns[4]) + 0);
      features[2] = strongOuter + (weakInner + (weakOuter + 0));
      features[6] = turns[2];
      features[8] = strongOuter;
      features[15] = Number(peak);
      features[16] = minimum;
      if (classifyCornerFeatures(features)) {
        geometry.nodes[region.contour[sample]].kind = 3;
        region.boundary[sample][0] |= 1;
      }
    }
  }
  return geometry;
}
