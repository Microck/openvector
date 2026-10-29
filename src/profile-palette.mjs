import {readPaletteColors,quantizeQColorChannels} from './pixel-input.mjs';

function colors(palette) {
  if ((Array.isArray(palette) || palette instanceof Float32Array) && palette.length === 0) return new Float32Array();
  return readPaletteColors(palette);
}

export function readPaletteRuntime(input) {
  if (input === null || typeof input !== 'object' || Array.isArray(input) || typeof input.customized !== 'boolean')
    throw new Error('Expected palette runtime state.');
  return {backup:colors(input.backup),customized:input.customized};
}

/** Mutate one owned selection and its separate runtime revert copy. The
 * profile can be the full settings record or the palette fields owned by a
 * browser draft. No mathematical document task runs for these GUI commands. */
export function applyPaletteCommand(profile,runtime,command) {
  if (command === null || typeof command !== 'object' || Array.isArray(command)) throw new Error('Choose a palette command.');
  switch (command.action) {
    case 'count': {
      const {count} = command;
      if (!Number.isInteger(count) || count < 2 || count > 24) throw new Error('Number of colors must be an integer from 2 through 24.');
      const changed = profile.colorCount !== count;profile.colorCount = count;return changed;
    }
    case 'replace':
    case 'choose': {
      const selected = quantizeQColorChannels(colors(command.palette));
      if (command.action === 'choose') applyPaletteCommand(profile,runtime,{action:'count',count:selected.length/4});
      profile.settings.palette = selected;
      profile.paletteColorCount = selected.length/4;
      runtime.backup = selected.slice();runtime.customized = false;
      break;
    }
    case 'color': {
      const selected = readPaletteColors(command.color);
      if (selected.length !== 4) throw new Error('A palette color needs four BGRA channels.');
      if (!Number.isInteger(command.index)) throw new Error('Choose an integer palette color index.');
      if (command.index >= 0 && command.index < profile.settings.palette.length/4)
        profile.settings.palette.set(quantizeQColorChannels(selected),command.index*4);
      runtime.customized = true;
      break;
    }
    case 'add':
      if (profile.settings.palette.length/4 >= 25) return;
      profile.settings.palette = Float32Array.of(...profile.settings.palette,1,1,1,1);
      profile.paletteColorCount = profile.settings.palette.length/4;
      runtime.customized = true;
      break;
    case 'delete': {
      const {index} = command,palette = profile.settings.palette;
      if (!Number.isInteger(index) || index < 0 || index >= palette.length/4) throw new Error('Choose an existing palette color.');
      profile.settings.palette = Float32Array.from([...palette.slice(0,index*4),...palette.slice(index*4+4)]);
      // The native delete slot deliberately does not write paletteColorCount.
      runtime.customized = true;
      break;
    }
    case 'revert':
      profile.settings.palette = runtime.backup.slice();profile.paletteColorCount = runtime.backup.length/4;
      runtime.customized = false;
      break;
    case 'source':
      profile.settings.palette = new Float32Array();profile.paletteColorCount = 2;
      runtime.customized = false;
      break;
    default: throw new Error('Choose count, replace, choose, color, add, delete, revert or source for the palette.');
  }
}
