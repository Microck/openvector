import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {PaletteSelection} from '../palette-selection.mjs';
import {AutoUpdate,autoUpdatePages} from '../auto-update.mjs';
import {GuiEngine} from '../engine.mjs';
import {exportSvg} from '../../src/svg.mjs';

const [, ...references] = gunzipSync(readFileSync(new URL('../../test/fixtures/palette-controls-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

for (const reference of references) test('native palette selection: '+reference.id,() => {
  const gui = new GuiEngine(),automatic = new AutoUpdate();
  const opened = gui.open({name:reference.input.name,bytes:Buffer.from(reference.input.bytes,'base64')});
  let state = gui.automatic({settings:opened.settings}),settings = state.settings,now = 0,completed = 0;
  const palette = new PaletteSelection({count:state.count,palette:settings.palette,
    paletteColorCount:state.paletteColorCount,runtime:state.paletteRuntime});
  const update = () => {
    const count = palette.snapshot().count;state = gui.palettes({settings,lower:count,upper:count,count});
    palette.replace(state.palettes.find(candidate=>candidate.count===count).colors.flat());
  };
  const compare = expected => {
    const selected = palette.snapshot(),bgra = selected.palette;
    const argb = Float32Array.from(bgra,(_,index)=>bgra[Math.floor(index/4)*4+3-index%4]);
    assert.equal(Buffer.from(argb.buffer).toString('hex'),expected.paletteArgbBits,'selected QColor channels');
    assert.equal(selected.count,Buffer.from(expected.selectedSettingsBits,'hex').readInt32LE(8),'requested count');
    assert.equal(selected.paletteColorCount,Buffer.from(expected.selectedSettingsBits,'hex').readInt32LE(56),'palette count bookkeeping');
    if (expected.paletteControls) assert.equal(Number(selected.customized),expected.paletteControls.header[5],'customization');
    if (expected.autoUpdate) assert.deepEqual(Object.values(automatic.snapshot()).map(widget=>
      [Number(widget.checked),Number(widget.enabled),Number(widget.pending),Number(widget.active)]),
      expected.autoUpdate.widgets.map(widget=>widget.fields.slice(1,5)),'manual edits versus automatic triggers');
  };
  compare(reference.expectedFrames[0]);
  for (const [index,action] of reference.actions.entries()) {
    if (action.action === 19) {
      if (action.slot === 'setUseAdvancedMode(bool)') {settings={...settings,advanced:Boolean(action.value)};automatic.settingsChanged(now);}
      else if (action.slot === 'setAdvNumColors(int)') {if (palette.setCount(action.value)) automatic.settingsChanged(now);}
      else if (action.slot === 'addColor()') palette.add();
      else if (action.slot === 'deleteColor(int)') palette.delete(action.value);
      else if (action.slot === 'revertPaletteEdits()') palette.revert();
      else if (action.slot === 'setPaletteFromList(int)') palette.choose(state.palettes.find(candidate=>candidate.count===action.value+1).colors.flat());
      else assert.fail('Unknown declared palette slot '+action.slot);
    } else if (action.action === 29) {
      const page = Object.keys(autoUpdatePages).find(page=>autoUpdatePages[page]===action.value);
      if (page === 'palette' && settings.paletteMode === 0) settings={...settings,paletteMode:1};
      automatic.enterPage(page,settings,state.stageKeys,now);
    } else if (action.action === 36) palette.color(action.index,action.argb.slice().reverse());
    else if (action.action === 32) {
      now += action.milliseconds;
      for (const page of automatic.advance(now)) {assert.equal(page,'palette');assert.equal(palette.snapshot().customized,false);update();}
    } else if (action.action === 37) {
      assert.equal(action.task,0);assert.equal(palette.snapshot().customized,true);
      if (action.answer === 0x400) update();
    } else assert.ok([31,35,38].includes(action.action));
    settings={...settings,palette:palette.snapshot().palette};
    if ([32,37].includes(action.action)) {
      const expected=reference.expectedCompleted[completed++].state;
      assert.equal(state.committedCount,Buffer.from(expected.settingsBits,'hex').readInt32LE(8),'committed count');
      assert.deepEqual(state.stageKeys,{palette:expected.cacheKey,...expected.stageKeys});
      assert.equal(state.svg,expected.fittedGeometry ? exportSvg({width:state.width,height:state.height,geometry:expected.fittedGeometry}) : null);
    }
    compare(reference.expectedFrames[index+1]);
  }
  assert.equal(completed,reference.expectedCompleted.length);
});

test('palette selection owns snapshots and rejects invalid counts and colors before mutation',() => {
  const selection = new PaletteSelection({palette:[0,0,0,1]});const original=selection.snapshot();
  original.palette[0]=1;assert.equal(selection.snapshot().palette[0],0);
  for (const count of [1,25,NaN]) assert.throws(()=>selection.setCount(count),/2 through 24/);
  assert.throws(()=>selection.color(0,[0,NaN,0,1]),/finite/);assert.equal(selection.snapshot().customized,false);
});
