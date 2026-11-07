import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {CoreSession} from '../../src/settings.mjs';
import {decodeImage,resolveImageFilename} from '../../src/image-input.mjs';
import {exportSvg} from '../../src/svg.mjs';
import {cleanStartupProfile} from './native-gui.mjs';

const tools = {0:'pan',3:'pencil',4:'picker',5:'fill',6:'zap'};

/** Replay only encoded input and declared controls/actions through real JS
 * application state. Captured expectations never enter the computation.
 * Returned geometry owns straight colors, including any user-painted colors. */
export function replayApplicationEdits({input,actions,controls = cleanStartupProfile.controls}, {gesture = false} = {}) {
  const {shadowControl,extension} = resolveImageFilename(input.name);
  const document = new CoreSession().createDocument({...decodeImage(Buffer.from(input.bytes,'base64')),shadowControl});
  const automatic = document.runAutomatic({...structuredClone(controls),extension});
  document.beginEditingForOutput();
  for (const action of actions) {
    if (!gesture && action.action === 1) {
      const color = action.argb.slice().reverse();
      document.edit(action.kind === 3
        ? {tool:'pencil',from:action.coordinates.slice(0,2),to:action.coordinates.slice(2),color,commit:action.commit}
        : {tool:tools[action.kind],x:action.x,y:action.y,color,commit:action.commit});
    } else if (gesture && action.action === 6)
      document.gesture({phase:'select',tool:tools[action.tool]});
    else if (gesture && action.phase)
      document.gesture({phase:action.phase,point:action.point,buttons:action.buttons,button:action.button,modifiers:action.modifiers});
    else if (gesture && action.action === 11)
      document.gesture({phase:'color',color:action.argb.slice().reverse()});
    else if (action.action === 2) document.undoEdit();
    else if (action.action === 3) document.redoEdit();
    else if (action.action === 4) document.revertEdits();
    else if (action.action === 5) document.finishEditing(automatic.settings);
    else if (action.action === 10) document.beginEditingForOutput();
    else throw new Error('Unknown application edit reference action '+action.action);
  }
  // CLI plans also finish after a final paint/undo. Cache hits retain the fit.
  const state = document.finishEditing(automatic.settings);
  if (!state.fitting?.complete) throw new Error('Application edit reference did not complete fitting.');
  return {geometry:document.presentationGeometry(),width:state.segmentation.width,height:state.segmentation.height};
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv.length !== 3 || process.argv[2] !== '--direct-stdin')
    throw new Error('Usage: node test/helpers/application-edit-reference.mjs --direct-stdin');
  const recipe = JSON.parse(readFileSync(0,'utf8'));
  process.stdout.write(exportSvg({...replayApplicationEdits(recipe),layout:'stacked'}));
}
