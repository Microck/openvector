import { createContourSmoothingState, initializeSmoothingNeighborhoods, initializeContourCoordinates } from './contour-initialization.mjs';
import { createContourImageModel } from './image-measurement.mjs';
import { smoothingOptimize } from './smoothing.mjs';
import { punctureContourCorners } from './contour-corners.mjs';
import { createContourRepair } from './smoothing-repair.mjs';

// Float32 table entries promoted to double, recovered from the original
// smoothing tables at 0xa22f98 and 0xa22f74. Substep 2 spans 0..500.
const smoothingRanges = [
  {'-1':[0.03551600128412247,0.03551600128412247],0:[0.035645999014377594,0.03569699823856354],
    1:[0.03570299968123436,0.03570299968123436],2:[0.03896399959921837,0.9999909996986389],
    3:[0.9999949932098389,0.9999949932098389],'-2':[0.9999970197677612,0.9999970197677612]},
  {'-1':[0.0037529999390244484,0.0037529999390244484],0:[0.004172999877482653,0.026402000337839127],
    1:[0.02749899961054325,0.02749899961054325],2:[0.03209700062870979,0.9967300295829773],
    3:[0.9967319965362549,0.9967319965362549],'-2':[0.9999989867210388,0.9999989867210388]},
];

function writePositions(geometry, positions) {
  for (let node = 0; node < geometry.nodes.length; node++) {
    geometry.nodes[node].x = positions[node * 2];
    geometry.nodes[node].y = positions[node * 2 + 1];
  }
}

// Internal session-owned entry: callers cannot reset or replace the public
// application's random stream through the package API.
export function smoothContourGeometry(input, nextUniform, onProgress) {
  const { parameters } = input;
  const antiAlias = parameters?.['Shared::is_anti_aliased'];
  const enabled = parameters?.['ContourSmoother::is_enabled'];
  const puncture = parameters?.['ContourSmoother::do_puncture_corners'];
  const measurements = parameters?.['ContourSmoother::measurement_types'];
  for (const [name, values] of [['is_enabled', enabled], ['do_puncture_corners', puncture]]) {
    if (!Array.isArray(values) || values.length !== 3 || Array.from(values).some(value => value !== 0 && value !== 1)) {
      throw new Error(`${name} must contain three phase flags.`);
    }
  }
  if ((antiAlias !== 0 && antiAlias !== 1) || !Array.isArray(measurements) || measurements.length !== 3) {
    throw new Error('Full smoothing needs numeric anti-alias and measurement controls.');
  }
  const prepared = createContourSmoothingState({ ...input, antiAlias:Boolean(antiAlias) });
  let geometry = prepared.geometry, penaltyCounts = prepared.penaltyCounts;
  let previousSubstep, previousCounter;
  const checkpoint = (substep, counter = 0) => {
    if (substep === previousSubstep && counter === previousCounter) return;
    previousSubstep = substep;previousCounter = counter;
    const [start,end] = smoothingRanges[antiAlias][substep], upper = substep === 2 ? 500 : 1;
    const fraction = counter <= 0 ? start : counter >= upper ? end : (end-start)*(counter/upper)+start;
    onProgress?.({phase:8,substep,counter,fraction,percent:Math.floor(fraction*100+0.5)},
      {geometry,penaltyCounts,borderCounts:prepared.borderCounts,referencePositions:prepared.referencePositions});
  };
  checkpoint(-1);
  checkpoint(0);
  initializeSmoothingNeighborhoods(prepared, Boolean(antiAlias));
  checkpoint(0,1);
  const imageModel = antiAlias || measurements.includes(1) ? createContourImageModel({ ...input, geometry }) : undefined;
  const initial = initializeContourCoordinates({ ...input, ...prepared }, nextUniform);
  writePositions(geometry, initial.positions);
  checkpoint(1);
  const budgets = Array.from({length:3}, (_,phase) => enabled[phase] ? parameters[`ContourSmoother::phase_${phase}.cg_max_iter`] : 0);
  const totalBudget = budgets.reduce((sum,budget) => (sum+budget)|0,0);
  // 0x46f7f0 multiplies in int32, divides, adds 0.5, truncates through an
  // int64 and dispatches its low DWORD. For 0/0 the native integer-indefinite
  // value has low DWORD zero, matching the final ToInt32 here.
  const progressCounter = completed => Math.trunc(((completed*500)|0)/totalBudget+0.5)|0;
  let precedingBudget = 0;
  for (let phase = 0; phase < 3; phase++) {
    if (enabled[phase]) {
      const observeIteration = onProgress === undefined ? undefined : (iteration,state) => {
        for (const [node,point] of state.points.entries()) {
          geometry.nodes[node].x = point[0];geometry.nodes[node].y = point[1];
          geometry.nodes[node].flags = state.geometry.nodes[node].flags;
        }
        penaltyCounts.set(state.penaltyCounts);
        checkpoint(2,progressCounter((precedingBudget+iteration)|0));
      };
      const optimized = smoothingOptimize({ ...input, geometry, penaltyCounts, phase, imageModel,
        referencePositions:prepared.referencePositions, borderCounts:prepared.borderCounts,
        nearBorderNodes:prepared.nearBorderNodes,
        ...(antiAlias ? { nearbyEdges:prepared.nearbyEdges, repairInterval:prepared.repairInterval } : {}) }, observeIteration);
      writePositions(geometry, optimized.positions);
      if (optimized.repairState) {
        penaltyCounts.set(optimized.repairState.penaltyCounts);
        for (let node = 0; node < geometry.nodes.length; node++) geometry.nodes[node].flags = optimized.repairState.flags[node];
      }
      if (puncture[phase]) geometry = punctureContourCorners({ geometry });
    }
    precedingBudget = (precedingBudget+budgets[phase])|0;
    checkpoint(2,progressCounter(precedingBudget));
  }
  checkpoint(3);
  if (antiAlias) {
    // Original 0x497c30 ends with one all-region repair, including decay,
    // even when no optimization phase is enabled.
    const state = { geometry, penaltyCounts, points:geometry.nodes.map(node => [node.x, node.y]) };
    createContourRepair(state, prepared.nearbyEdges).all();
  }
  checkpoint(-2);
  return { geometry, penaltyCounts, borderCounts:prepared.borderCounts, referencePositions:prepared.referencePositions };
}
