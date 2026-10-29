import progressData from './segmentation-progress-data.json' with {type:'json'};
import { segmentWithMoments } from './segmentation.mjs';

/** Document-owned live labels/palette and separately retained exported records.
 * The lower kernels own all arithmetic. Only their actual producers update
 * this view; absent labels remain unavailable rather than exposing allocation. */
export class SegmentationState {
  #state;

  constructor(previous,width,height,palette) {
    const sameSize = previous?.width === width && previous.height === height;
    this.#state = {width,height,indices:sameSize ? previous.indices?.slice() ?? null : null,
      palette:palette?.slice() ?? previous?.palette.slice() ?? new Float32Array(),
      counts:previous?.counts.slice() ?? new Int32Array(),
      colorIndices:previous?.colorIndices.slice() ?? new Int32Array()};
  }

  run(input,onProgress) {
    const profile = `segmentation-${input.palette === undefined ? 'unlimited' : 'fixed'}-${input.parameters['Shared::is_anti_aliased'] ? 'antialiased' : 'aliased'}`;
    const ranges = progressData.profiles[profile];
    let previousPhase,previousSubstep,previousCounter;
    const checkpoint = (phase,substep,counter = 0,updated) => {
      if (updated) Object.assign(this.#state,updated);
      if (phase === previousPhase && substep === previousSubstep && counter === previousCounter) return;
      previousPhase = phase;previousSubstep = substep;previousCounter = counter;
      const range = ranges[`${phase}:${substep}`];
      if (!range) throw new Error(`Unrecovered segmentation progress range: ${profile}/${phase}:${substep}.`);
      const [start,end,lower,upper] = range;
      const fraction = counter <= lower ? start : counter >= upper ? end : (end-start)*((counter-lower)/(upper-lower))+start;
      onProgress({phase,substep,counter,fraction,percent:Math.floor(fraction*100+0.5)},this.#state);
    };
    checkpoint(2,-1);
    segmentWithMoments(input,input,checkpoint);
    return this.#state;
  }
}
