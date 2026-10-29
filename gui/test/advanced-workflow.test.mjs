import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {AutoUpdate,advancedControlsMatch} from '../auto-update.mjs';
import {PaletteSelection} from '../palette-selection.mjs';
import {exportSvg} from '../../src/svg.mjs';

const [, ...references] = gunzipSync(readFileSync(new URL('../../test/fixtures/advanced-workflow-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const argbBits=bgra=>{
  const argb=Float32Array.from(bgra,(_,index)=>bgra[Math.floor(index/4)*4+3-index%4]);
  return Buffer.from(argb.buffer).toString('hex');
};
const tools={0:'pan',3:'pencil'};

for (const reference of references) test('native advanced workflow: '+reference.id,()=>{
  const gui=new GuiEngine(),automatic=new AutoUpdate();
  let state=gui.open({name:reference.input.name,bytes:Buffer.from(reference.input.bytes,'base64')});
  let page=4,now=0,completed=0;
  if (reference.expectedAutomatic) {
    state=gui.automatic({settings:state.settings});gui.edit({action:'begin'});state=gui.background({action:'begin'});page=15;
  }
  let settings=state.settings;
  const palette=new PaletteSelection({count:state.count,palette:settings.palette,
    paletteColorCount:state.paletteColorCount,runtime:state.paletteRuntime});
  const compareCompleted=expected=>{
    assert.deepEqual(state.stageKeys,expected.stageKeys,'committed stage keys');
    assert.equal(state.committedCount,expected.count,'committed advanced count');
    assert.equal(state.svg,expected.geometry ? exportSvg({width:state.width,height:state.height,geometry:expected.geometry}) : null);
    assert.deepEqual(state.palettes.map(candidate=>({count:candidate.count,argbBits:argbBits(candidate.colors.flat()),
      scoreBits:Buffer.from(Float32Array.of(candidate.score).buffer).toString('hex')})),expected.palettes,'independent candidate bytes and scores');
  };
  if (reference.expectedAutomatic) compareCompleted(reference.expectedAutomatic);
  const compareFrame=expected=>{
    assert.equal(page,expected.page);assert.equal(palette.snapshot().count,expected.count);
    assert.equal(argbBits(palette.snapshot().palette),expected.paletteArgbBits,'selected QColor palette');
    assert.deepEqual(state.segmentationHistory,{undoCount:expected.undo.length,redoCount:expected.redo.length});
    if (state.editor) {
      assert.equal(state.editor.key,expected.key);assert.equal(state.editor.tool,tools[expected.tool]);
      const rgba=Buffer.from(state.segments.rgba,'base64');
      for (let offset=0;offset<rgba.length;offset+=4) [rgba[offset],rgba[offset+2]]=[rgba[offset+2],rgba[offset]];
      assert.equal(rgba.toString('hex'),expected.bgra,'displayed segmentation pixels');
    }
    if (expected.autoUpdate) assert.deepEqual(Object.values(automatic.snapshot()).map((widget,index)=>
      [[372,400,432][index],Number(widget.checked),Number(widget.enabled),Number(widget.pending),Number(widget.active),500,1]),expected.autoUpdate);
  };
  compareFrame(reference.expectedFrames[0]);
  for (const [index,action] of reference.actions.entries()) {
    if (action.action===29 && action.slot==='setWizardMode(VmWizardModeType)') settings={...settings,advanced:true};
    else if (action.action===29 && action.slot==='processNextPress()') {
      if (page===4) {
        settings={...settings,paletteMode:1};
        if (state.file.hasAlpha) settings={...settings,backgroundMode:0,background:[1,1,1,0]};
        page=state.file.hasAlpha ? 6 : 12;
      } else {assert.equal(page,6);page=12;}
      automatic.enterPage(page===12 ? 'palette' : null,settings,state.stageKeys,now);
    } else if (action.action===32) {
      now+=action.milliseconds;
      for (const stage of automatic.advance(now)) {
        if (stage==='palette') {
          const count=palette.snapshot().count;state=gui.palettes({settings,lower:count,upper:count,count});
          palette.replace(state.palettes.find(candidate=>candidate.count===count).colors.flat());settings={...settings,palette:palette.snapshot().palette};
        } else {assert.equal(stage,'smoothing');state=gui.vectors({settings});}
      }
      compareCompleted(reference.expectedCompleted[completed++]);
    } else if (action.action===10) {state=gui.edit({action:'begin'});page=14;}
    else if (action.action===19) {assert.equal(action.slot,'setUseAdvancedMode(bool)');settings={...settings,advanced:Boolean(action.value)};}
    else if (action.action===6) state=gui.edit({action:'gesture',events:[{phase:'select',tool:tools[action.tool]}]});
    else if (action.action===11) state=gui.edit({action:'gesture',events:[{phase:'color',color:action.argb.slice().reverse()}]});
    else if (action.action===22) {
      // These native view records come from a single pointer press. The direct
      // CLI command has different origin fields in its serialized history key.
      assert.equal(action.kind,3);assert.equal(action.commit,true);
      assert.deepEqual(action.coordinates.slice(0,2),action.coordinates.slice(2));
      state=gui.edit({action:'gesture',events:[{phase:'press',point:action.coordinates.slice(0,2),buttons:1,button:1}]});
    } else if (action.action===23) {
      state=gui.edit({action:'finish',settings});page=16;automatic.enterPage('smoothing',settings,state.stageKeys,now);
      compareCompleted(reference.expectedCompleted[completed++]);
    } else if (action.action===29 && action.slot==='processBackPress()') {
      if (page===16) {state=gui.edit({action:'begin'});page=14;automatic.enterPage(null,settings,state.stageKeys,now);}
      else if (state.editor.key!==state.stageKeys.segmentationEdits) state=gui.edit({action:'restore'});
      else {state=gui.edit({action:'leave'});page=13;automatic.enterPage('segmentation',settings,state.stageKeys,now);}
    } else assert.ok([31,35].includes(action.action),'known observation');
    compareFrame(reference.expectedFrames[index+1]);
  }
  assert.equal(completed,reference.expectedCompleted.length);
});

test('advanced dirty comparison includes only the native eleven-field control block',()=>{
  const settings=new GuiEngine().open({name:references[0].input.name,bytes:Buffer.from(references[0].input.bytes,'base64')}).settings;
  for (const field of ['sensitivity','antiAlias','complexity','minimumPixels','artifactRejection','clusterColors',
    'smoothness','detectCorners','curveComplexity','smoothingAntiAlias']) {
    const changed=typeof settings[field]==='boolean' ? !settings[field] : settings[field]+1;
    assert.equal(advancedControlsMatch({...settings,[field]:changed},2,settings,2),false,field);
  }
  assert.equal(advancedControlsMatch(settings,3,settings,2),false);
  assert.equal(advancedControlsMatch({...settings,advanced:true,category:0,quality:2,paletteMode:2,palette:[0,0,0,1],backgroundMode:0},2,settings,2),true);
});
