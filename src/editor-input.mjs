import {readPaletteColors,quantizeQColorChannels} from './pixel-input.mjs';

export function readEditorPoint(point) {
  if (!Array.isArray(point) || point.length !== 2 || point.some(value => !Number.isFinite(value)))
    throw new Error('Gesture point must contain two finite world coordinates.');
  return point.slice();
}

/** The two editors share pointer/color wire rules; each owns its tool set.
 * Validate once here before either owner changes pixels or visibility. */
export function readEditorGesture(input,tools) {
  if (!input || typeof input !== 'object') throw new Error('Expected an editor gesture.');
  const {phase} = input;
  if (phase === 'select') {
    if (!tools.includes(input.tool)) throw new Error('Gesture tool must be '+tools.join(', ')+'.');
    return {phase,tool:input.tool};
  }
  if (phase === 'color') {
    const color = readPaletteColors(input.color);
    if (color.length !== 4) throw new Error('Gesture color must contain four BGRA channels.');
    return {phase,color:quantizeQColorChannels(color)};
  }
  if (!['press','move','release'].includes(phase)) throw new Error('Gesture phase must be select, color, press, move or release.');
  const {point,buttons,button = 0,modifiers = 0} = input;
  const position = readEditorPoint(point);
  for (const [name,value] of [['buttons',buttons],['button',button],['modifiers',modifiers]])
    if (!Number.isInteger(value) || value < 0 || value > 4294967295) throw new Error(`Gesture ${name} must be an unsigned int32 bit mask.`);
  return {phase,point:position,buttons,modifiers};
}
