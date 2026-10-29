import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {AutoUpdate,autoUpdatePages,pageNeedsUpdate} from '../auto-update.mjs';
import {GuiEngine} from '../engine.mjs';
import {exportSvg} from '../../src/svg.mjs';

const [, ...references] = gunzipSync(readFileSync(new URL('../../test/fixtures/auto-update-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [, ...reentries] = gunzipSync(readFileSync(new URL('../../test/fixtures/auto-update-reentry-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const properties = {'setUseAdvancedMode(bool)':['advanced',Boolean],
  'setAdvContourSmoothness(int)':['smoothness',Number],'setAdvCurveComplexity(int)':['curveComplexity',Number],
  'setAdvColorSensitivity(int)':['sensitivity',Number],'setAdvSegmentationComplexity(int)':['complexity',Number]};
const offsets = {372:'palette',400:'segmentation',432:'smoothing'};

for (const reference of [...references,...reentries]) test('native Auto-Update dispatch: '+reference.id,() => {
  const gui = new GuiEngine(),automatic = new AutoUpdate();
  const opened = gui.open({name:reference.input.name,bytes:Buffer.from(reference.input.bytes,'base64')});
  let state = gui.automatic({settings:opened.settings}),settings = state.settings,now = 0,completed = 0;
  const compare = (expected,label) => {
    assert.deepEqual(state.stageKeys,expected.stageKeys,label+' retained cache keys');
    assert.equal(state.svg,expected.geometry ? exportSvg({width:state.width,height:state.height,geometry:expected.geometry}) : null,
      label+' independently processed vector');
  };
  compare(reference.expectedAutomatic,reference.id+' automatic');
  for (const [index,action] of reference.actions.entries()) {
    const label = reference.id+' action '+index;
    if (action.action === 19) {
      const [key,convert] = properties[action.slot],value = convert(action.value);
      if (settings[key] !== value) {settings = {...settings,[key]:value};automatic.settingsChanged(now);}
    } else if (action.action === 29) {
      assert.equal(action.slot,'setWizardPage(WizardPageIdType)');
      const page = Object.keys(autoUpdatePages).find(page => autoUpdatePages[page] === action.value) ?? String(action.value);
      // updateWizardView normalizes the two-color palette to fixed before
      // qtWizardPageChanged enables the page's automatic checkbox.
      if (page === 'palette' && settings.paletteMode === 0) settings = {...settings,paletteMode:1};
      automatic.enterPage(page,settings,state.stageKeys,now);
    } else if (action.action === 33) {
      assert.equal(action.slot,'click()');automatic.click(offsets[action.offset],now);
    } else if (action.action === 32) {
      now += action.milliseconds;
      for (const page of automatic.advance(now)) {
        if (page === 'smoothing') state = gui.vectors({settings});
        else if (page === 'segmentation') state = gui.process({settings,through:'segmentation'});
        else {
          // These captures request the native Number of Colors default, two.
          // No expected native palette supplies the independent computation.
          state = gui.palettes({settings,lower:2,upper:2});
        }
      }
      compare(reference.expectedCompleted[completed++],label);
    } else assert.ok([24,31].includes(action.action),label+' known observation');
    const expected = reference.expectedFrames[index];
    if (expected) assert.deepEqual(Object.values(automatic.snapshot()).map((widget,widgetIndex) =>
      [Number(Object.keys(offsets)[widgetIndex]),Number(widget.checked),Number(widget.enabled),Number(widget.pending),
        Number(widget.active),500,1]),expected,label+' widget state');
  }
  assert.equal(completed,reference.expectedCompleted.length);
});

test('native cache predicates distinguish processing stages and do not use geometry availability',() => {
  const gui = new GuiEngine(),input = references[0].input;
  const opened = gui.open({name:input.name,bytes:Buffer.from(input.bytes,'base64')});
  const state = gui.automatic({settings:opened.settings}),settings = {...state.settings,advanced:true};
  const updated = gui.vectors({settings});
  assert.equal(pageNeedsUpdate('smoothing',settings,updated.stageKeys),false);
  assert.equal(pageNeedsUpdate('segmentation',settings,updated.stageKeys),true);
  assert.equal(pageNeedsUpdate('smoothing',{...settings,curveComplexity:9},updated.stageKeys),true);
  assert.equal(pageNeedsUpdate('smoothing',settings,{...updated.stageKeys,segmentationEdits:'pencil edit'}),true);
  assert.equal(pageNeedsUpdate('palette',settings,updated.stageKeys),false);
  assert.equal(pageNeedsUpdate('palette',{...settings,palette:[]},updated.stageKeys),true);
  assert.equal(pageNeedsUpdate('palette',{...settings,palette:[],paletteMode:2},updated.stageKeys),false);
});
