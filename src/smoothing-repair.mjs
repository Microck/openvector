// Trusted prepared-state kernel. The complete smoother owns geometry, points
// and byte penalties; nearby associations stay fixed while coordinates move.
function unitDirection(x, y) {
  const length = Math.sqrt(y * y + x * x);
  if (length > 0) {
    const inverse = 1 / length;
    x *= inverse;
    y = inverse * y;
  }
  return [x, y];
}

function jointDirection(previous, current, following) {
  const incoming = unitDirection(previous[0] - current[0], previous[1] - current[1]);
  const outgoing = unitDirection(following[0] - current[0], following[1] - current[1]);
  let x = incoming[1] + -outgoing[1], y = -incoming[0] + outgoing[0];
  const sumX = outgoing[0] + incoming[0], sumY = outgoing[1] + incoming[1];
  if (y * y + x * x < sumY * sumY + sumX * sumX) {
    x = sumX; y = sumY;
    if (outgoing[1] * incoming[0] - incoming[1] * outgoing[0] > 0) { x = -x; y = -y; }
  }
  return unitDirection(x, y);
}

function windingContribution(origin, direction, first, last) {
  const denominator = (last[1] - first[1]) * direction[0] - (last[0] - first[0]) * direction[1];
  if (denominator !== 0) {
    if (Math.abs((last[1] - origin[1]) * direction[0] - (last[0] - origin[0]) * direction[1]) < 0.0001 ||
        Math.abs((first[1] - origin[1]) * direction[0] - (first[0] - origin[0]) * direction[1]) < 0.0001) {
      return denominator <= 0 ? -1 : 1;
    }
    const alongRay = (last[0] - first[0]) * (origin[1] - first[1]) -
      (last[1] - first[1]) * (origin[0] - first[0]);
    const alongEdge = (origin[1] - first[1]) * direction[0] - (origin[0] - first[0]) * direction[1];
    if (denominator <= 0) {
      if (alongRay <= 0 && alongEdge < 0 && denominator < alongEdge) return -2;
    } else if (alongRay >= 0 && alongEdge > 0 && alongEdge < denominator) return 2;
  }
  return 0;
}

function onSegment(point, first, last, direction) {
  const dx = point[0] - first[0], dy = point[1] - first[1];
  if (Math.abs(dy * direction[0] - dx * direction[1]) >= 0.0001) return false;
  const projection = dy * direction[1] + dx * direction[0];
  const outside = projection < 0 || Math.sqrt((last[1] - first[1]) * (last[1] - first[1]) +
    (last[0] - first[0]) * (last[0] - first[0])) < projection;
  return !(outside && dy * dy + dx * dx >= 1e-8 &&
    (point[1] - last[1]) * (point[1] - last[1]) + (point[0] - last[0]) * (point[0] - last[0]) >= 1e-8);
}

function crossing(first, last, otherFirst, otherLast) {
  let denominator = (otherLast[1] - otherFirst[1]) * (last[0] - first[0]) -
    (otherLast[0] - otherFirst[0]) * (last[1] - first[1]);
  if (denominator === 0) return 0;
  const sign = denominator >= 0 ? 1 : -1;
  const direction = unitDirection(last[0] - first[0], last[1] - first[1]);
  const otherDirection = unitDirection(otherLast[0] - otherFirst[0], otherLast[1] - otherFirst[1]);
  if (onSegment(first, otherFirst, otherLast, otherDirection) || onSegment(last, otherFirst, otherLast, otherDirection) ||
      onSegment(otherFirst, first, last, direction) || onSegment(otherLast, first, last, direction)) return sign;
  let firstNumerator = (otherLast[0] - otherFirst[0]) * (first[1] - otherFirst[1]) -
    (otherLast[1] - otherFirst[1]) * (first[0] - otherFirst[0]);
  let lastNumerator = (last[0] - first[0]) * (first[1] - otherFirst[1]) -
    (last[1] - first[1]) * (first[0] - otherFirst[0]);
  if (denominator < 0) { denominator = -denominator; firstNumerator = -firstNumerator; lastNumerator = -lastNumerator; }
  return firstNumerator > 0 && firstNumerator < denominator && lastNumerator > 0 && lastNumerator < denominator ? sign * 2 : 0;
}

// Adjacent edges meet at one node. Native 0x4811a0 compares the four
// normalized rays around that joint instead of testing the shared endpoint.
function adjacentCrossing(first, following, otherFirst, otherFollowing, joint) {
  return crossing(...[first, following, otherFirst, otherFollowing].map(point =>
    unitDirection(point[0] - joint[0], point[1] - joint[1])));
}

export function createContourRepair(state, nearbyEdges) {
  const { nodes, regions } = state.geometry;
  const probeDirections = regions.map(() => [0, 0]), windings = new Int32Array(regions.length);

  function seedWinding(regionId, sample) {
    const { contour } = regions[regionId], count = contour.length, points = state.points;
    const direction = jointDirection(points[contour[(sample + count - 1) % count]],
      points[contour[sample]], points[contour[(sample + 1) % count]]);
    probeDirections[regionId] = direction;
    let sum = 0;
    for (let edge = sample + 2; edge < sample + count; edge++) {
      sum += windingContribution(points[contour[sample]], direction,
        points[contour[(edge - 1) % count]], points[contour[edge % count]]);
    }
    // The native diagnostic resets an odd sum before returning its seed.
    if (sum % 2 !== 0) sum = 0;
    return 2 - sum;
  }

  function mark(node) {
    const changed = (nodes[node].flags & 2) === 0 ? 1 : 0;
    nodes[node].flags |= 0x16;
    state.penaltyCounts[node] = 3;
    return changed;
  }

  function region(regionId) {
    const { contour } = regions[regionId], count = contour.length, points = state.points;
    windings[regionId] = seedWinding(regionId, 0);
    for (const node of contour) nodes[node].flags &= 0xf0;
    let changed = 0, accumulated = 0, consecutive = 0;
    for (let sample = 0; sample < count; sample++) {
      const first = contour[sample], last = contour[(sample + 1) % count];
      let signed = 0, absolute = 0;
      for (const [otherId, otherSample] of nearbyEdges[first]) {
        const otherContour = regions[otherId].contour, otherCount = otherContour.length;
        const otherFirst = otherContour[otherSample], otherLast = otherContour[(otherSample + 1) % otherCount];
        if (last === otherLast && regionId === otherId) {
          const contribution = adjacentCrossing(points[first], points[contour[(sample + 2) % count]],
            points[otherFirst], points[otherContour[(otherSample + 2) % otherCount]], points[last]);
          absolute += Math.abs(contribution);
          signed += contribution;
        } else if (first !== otherFirst && first !== otherLast && last !== otherFirst && last !== otherLast) {
          const contribution = crossing(points[first], points[last], points[otherFirst], points[otherLast]);
          absolute += Math.abs(contribution);
          if (regionId === otherId) signed += contribution;
        }
      }
      accumulated += signed;
      if (absolute === 0) {
        if (windings[regionId] + accumulated === 0) {
          nodes[last].flags |= 4;
          consecutive = 0;
        } else {
          changed += mark(last);
          consecutive++;
          if (consecutive > 20) {
            windings[regionId] = seedWinding(regionId, sample);
            accumulated = 0;
            consecutive = -2147483647;
          }
        }
      } else {
        changed += mark(first);
        changed += mark(last);
      }
    }
    return changed;
  }

  function decay() {
    let changed = 0;
    for (let node = 0; node < nodes.length; node++) if (state.penaltyCounts[node] > 1) {
      state.penaltyCounts[node]--;
      changed++;
    }
    return changed;
  }

  function nodeState() {
    return { flags:nodes.map(node => node.flags), penaltyCounts:Array.from(state.penaltyCounts) };
  }

  return {
    region, decay,
    nodeState,
    invalid(identifiers) {
      let changed = 0;
      for (const identifier of identifiers) changed += region(identifier);
      return changed > 0;
    },
    all() {
      let changed = decay();
      for (let identifier = 0; identifier < regions.length; identifier++) changed += region(identifier);
      return changed;
    },
    snapshot() {
      return { ...nodeState(),
        probeDirections:probeDirections.map(direction => direction.slice()), windings:Array.from(windings) };
    },
  };
}
