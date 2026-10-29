import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {exportSvg} from '../../src/svg.mjs';
import {GuiEngine} from '../engine.mjs';
import {startIsolatedGui} from './gui-host.mjs';

const [, ...workflows] = gunzipSync(readFileSync(new URL('../../test/fixtures/background-edit-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [, ...previews] = gunzipSync(readFileSync(new URL('../../test/fixtures/background-preview-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const tools = {0:'pan',3:'pencil',4:'picker',5:'fill',6:'zap',7:'background'};
const properties = {'setUseAdvancedMode(bool)':['advanced',Boolean],
  'setAdvContourSmoothness(int)':['smoothness',Number],'setAdvCurveComplexity(int)':['curveComplexity',Number]};

for (const workflow of [...workflows,...previews]) test('GUI background reference: '+workflow.id,() => {
  const gui = new GuiEngine();
  const opened = gui.open({name:workflow.input.name,bytes:Buffer.from(workflow.input.bytes,'base64')});
  let state = gui.automatic({settings:opened.settings}),selected = state.settings,page = 'background',completed = 0;
  let currentGeometry = workflow.expectedAutomatic.state.fittedGeometry,scene = currentGeometry;
  gui.edit({action:'begin'});state = gui.background({action:'begin'});
  const compare = (expected,label) => {
    const hiddenRegions = expected.visibility.visible.flatMap((visible,id) => visible ? [] : [id]);
    assert.deepEqual(state.background,{count:expected.visibility.header[3],active:Boolean(expected.visibility.header[5]),
      tool:expected.visibility.header[5] ? tools[expected.inputMode] : 'pan',hiddenRegions,
      undo:expected.visibility.undo,redo:expected.visibility.redo},label+' visibility/history');
    const options = {width:state.width,height:state.height,hiddenRegions};
    assert.equal(state.svg,currentGeometry ? exportSvg({...options,geometry:currentGeometry}) : null,label+' export');
    assert.equal(state.preview.svg,exportSvg({...options,geometry:scene,layout:'cutouts'}),label+' retained preview');
    assert.equal(state.preview.outlines.length > 0,hiddenRegions.length > 0,label+' outline presence');
    if (page === 'editing') {
      assert.equal(state.editor.key,expected.key,label+' pixel edit history');
      assert.equal(state.editor.undoCount,expected.undo.length);assert.equal(state.editor.redoCount,expected.redo.length);
    }
  };
  compare(workflow.expectedFrames[0],workflow.id+' initial');
  for (const [index,action] of workflow.actions.entries()) {
    const expected = workflow.expectedFrames[index+1],operation = page === 'background' ? 'background' : 'edit';
    if (action.action === 6) state = gui[operation]({action:'gesture',events:[{phase:'select',tool:tools[action.tool]}]});
    else if (action.phase) state = gui[operation]({action:'gesture',events:[{phase:action.phase,point:action.point,
      buttons:action.buttons,button:action.button,modifiers:action.modifiers}]});
    else if (action.action === 10) {state = gui.edit({action:'begin'});page = 'editing';}
    else if (action.action === 11) state = gui.edit({action:'gesture',events:[{phase:'color',color:action.argb.slice().reverse()}]});
    else if (action.action === 12) {state = gui.background({action:'begin'});page = 'background';}
    else if ([13,14,15].includes(action.action)) state = gui.background({action:{13:'undo',14:'redo',15:'revert'}[action.action]});
    else if ([16,24].includes(action.action)) state = gui.state(); // Native observation actions do not mutate state.
    else if (action.action === 18) {
      const input = workflow.additionalInputs[action.source];
      const reloaded = gui.open({name:input.name,bytes:Buffer.from(input.bytes,'base64'),settings:selected});
      state = gui.automatic({settings:reloaded.settings});selected = state.settings;page = 'export';
      currentGeometry = expected.replacedSource.automatic.state.fittedGeometry;scene = currentGeometry;
    } else if (action.action === 19) {
      const [key,convert] = properties[action.slot];selected = {...selected,[key]:convert(action.value)};
    } else if (action.action === 21) state = gui.background({action:'toggle',point:action.point});
    else if (action.action === 22) {
      if (action.kind === 3) assert.deepEqual(action.coordinates.slice(0,2),action.coordinates.slice(2));
      const point = action.kind === 3 ? action.coordinates.slice(0,2) : [action.x+.5,action.y+.5];
      state = gui.edit({action:'gesture',events:[{phase:'press',point,buttons:1,button:1}]});
    } else if ([2,3,4].includes(action.action)) state = gui.edit({action:{2:'undo',3:'redo',4:'revert'}[action.action]});
    else if ([5,23].includes(action.action)) {
      if (page === 'editing') {
        state = gui.edit({action:'finish',settings:selected});page = selected.advanced ? 'smoothing' : 'export';
      } else if (page === 'smoothing') {
        state = gui.vectors({settings:selected});
        if (!state.didWork) {state = gui.background({action:'begin'});page = 'background';}
      }
      else {state = gui.background({action:'finish'});page = 'export';}
      currentGeometry = workflow.expectedCompleted[completed++].state.fittedGeometry;
      if (currentGeometry) scene = currentGeometry;
    } else assert.fail('Unknown native background command '+action.action);
    compare(expected,workflow.id+' action '+index);
  }
  assert.equal(completed,workflow.expectedCompleted.length);
});

test('GUI background HTTP batches preserve accepted prefixes and export current visibility',async t => {
  const host = await startIsolatedGui({port:0});t.after(() => host.close());
  const api = (action,input) => fetch(`${host.url}/api/${action}`,{method:'POST',
    headers:{'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'},body:JSON.stringify(input)});
  const input = workflows.find(row => row.id === 'background-square-queries').input;
  let response = await api('open',{name:input.name,bytes:input.bytes});let state = await response.json();
  response = await api('automatic',{settings:state.settings});state = await response.json();const original = state.svg;
  assert.equal((await api('background',{action:'begin'})).status,200);
  response = await api('background',{action:'gesture',events:[{phase:'press',point:[1.5,2],buttons:1},
    {phase:'move',point:[null,0],buttons:1}]});
  assert.equal(response.status,400);const rejected = await response.json();assert.match(rejected.error,/finite/);
  assert.equal(rejected.state.background.undo.length,1);assert.notEqual(rejected.state.svg,original);
  assert.ok(rejected.state.preview.outlines.length);
  const output = await fetch(`${host.url}/api/export?format=svg`,{headers:{'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'}});
  assert.equal(await output.text(),rejected.state.svg);
  response = await api('background',{action:'undo'});state = await response.json();assert.equal(state.svg,original);
  assert.equal(state.preview.outlines.length,0);
  assert.equal((await api('background',{action:'finish'})).status,200);
  response = await api('vectors',{settings:state.settings});assert.equal(response.status,200);
});
