import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {PaletteSelection} from '../palette-selection.mjs';
import {startIsolatedGui} from './gui-host.mjs';
import {SavedAdvancedProfiles,defaultAdvancedProfile} from '../../src/saved-profiles.mjs';
import {quantizeQColorChannels} from '../../src/pixel-input.mjs';
import {compareDocumentState,profileFromFrame,settingsFromBits} from '../../test/helpers/native-gui.mjs';

const [provenance,...references] = gunzipSync(readFileSync(new URL('../../test/fixtures/profile-palettes-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

test('fresh palette preparation matches the complete native pre-segmentation map without exposing completed regions',() => {
  const [parameters,...windows] = gunzipSync(readFileSync(new URL('../../test/fixtures/profile-replay-window-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const reference = windows.find(row => row.id === 'visible-window-palette-play');
  const source = reference.actions[0][1],engine = new GuiEngine();
  engine.open({name:source,bytes:Buffer.from(reference.inputs[source],'base64')});
  engine.applyProfile(defaultAdvancedProfile());
  const selected = engine.selectedProfile();
  const candidates = engine.palettes({settings:selected.settings,count:selected.colorCount,lower:2,upper:2});
  engine.palette({action:'replace',palette:candidates.palettes.find(candidate => candidate.count === 2).colors.flat()});
  const expected = reference.expected.at(-1).document.state;
  compareDocumentState(engine.snapshot().document,expected,parameters.parameters);
  assert.deepEqual(engine.selectedProfile(),profileFromFrame(reference.expected.at(-2).selected));
  const view = engine.state();
  assert.equal(view.stage,'image');assert.equal(view.regionCount,0);assert.equal(view.segments,null);assert.equal(view.svg,null);
  assert.throws(() => engine.edit({action:'begin'}),/completed segmentation/);
  assert.throws(() => engine.vectors({settings:engine.selectedProfile().settings}),/completed segmentation/);
});

for (const reference of references) test('native profile/palette runtime: '+reference.id,() => {
  const engine = new GuiEngine(),profiles = new SavedAdvancedProfiles();
  let selection = new PaletteSelection();
  for (const [index,action] of reference.actions.entries()) {
    const [kind,...values] = action,expected = reference.expected[index];
    if (kind === 'automatic' || kind === 'open') {
      const source = values[0];
      let state = engine.open({bytes:Buffer.from(reference.inputs[source],'base64'),name:source});
      if (kind === 'automatic') {
        state = engine.automatic({settings:state.settings});
        selection = new PaletteSelection({count:state.count,palette:state.settings.palette,
          paletteColorCount:state.paletteColorCount,runtime:state.paletteRuntime});
      } else {
        selection.applyProfile(engine.selectedProfile());selection.sourceLoaded();
      }
    } else if (kind === 'set') {
      const [receiver,signature,value] = values;assert.equal(receiver,2);
      if (signature === 'addColor()') {engine.palette({action:'add'});selection.add();}
      else if (signature === 'deleteColor(int)') {engine.palette({action:'delete',index:value});selection.delete(value);}
      else if (signature === 'revertPaletteEdits()') {engine.palette({action:'revert'});selection.revert();}
      else if (signature === 'setPaletteFromList(int)') {
        const colors = engine.state().palettes.find(candidate => candidate.count === value+1).colors.flat();
        engine.palette({action:'choose',palette:colors});selection.choose(colors);
      } else if (signature === 'setAdvNumColors(int)') {
        engine.palette({action:'count',count:value});selection.setCount(value);
      } else {
        const slots = {'setUseAdvancedMode(bool)':'advanced','setAdvSegmentationComplexity(int)':'complexity',
          'setAdvContourSmoothness(int)':'smoothness','setPaletteMode(int)':'paletteMode','setFlattenImageMode(int)':'backgroundMode'};
        const name = slots[signature];assert.ok(name,signature);
        engine.configureProfile({settings:{[name]:name === 'advanced' ? Boolean(value) : value}});
      }
    } else if (kind === 'color') {
      engine.configureProfile({settings:{background:quantizeQColorChannels(Float32Array.from(values[0]).reverse())}});
    } else if (kind === 'save') profiles.put(values[0],engine.selectedProfile());
    else if (kind === 'load') {
      const selected = profiles.select(values[0]);assert.ok(selected.profile);
      engine.applyProfile(selected.profile);selection.applyProfile(engine.selectedProfile());
    } else assert.ok(['snapshot','document'].includes(kind),kind);

    if (expected.selected) {
      const profile = profileFromFrame(expected.selected),runtime = expected.paletteRuntime;
      const backup = settingsFromBits(expected.selected.settingsBits,runtime.backupArgbBits).palette;
      assert.deepEqual(engine.selectedProfile(),profile,'full selected profile');
      assert.deepEqual(engine.paletteRuntime(),{backup:Array.from(backup),customized:runtime.customized},'actual engine runtime');
      assert.deepEqual(selection.runtime(),engine.paletteRuntime(),'browser and engine use the same transition kernel');
      assert.deepEqual(selection.snapshot(),{count:profile.colorCount,palette:Array.from(profile.settings.palette),
        paletteColorCount:profile.paletteColorCount,customized:runtime.customized},'complete browser palette selection');
      assert.deepEqual(profiles.snapshot(),expected.profiles.map(row => ({name:row.name,profile:profileFromFrame(row)})),
        'saved records retain independent palette counts');
    }
    if (kind === 'document') {
      const actual = engine.snapshot(),document = expected.document;
      compareDocumentState(actual.document,document.state,provenance.parameters);
      assert.equal(actual.committedCount,Buffer.from(document.state.settingsBits,'hex').readInt32LE(8));
    }
  }
});

test('palette transitions validate before mutation and own every returned runtime buffer',() => {
  const engine = new GuiEngine(),before = engine.snapshot();
  for (const command of [null,[],{action:'unknown'},{action:'count',count:25},{action:'choose',palette:[]},
    {action:'color',index:0,color:[0,NaN,0,1]},{action:'color',index:.5,color:[0,0,0,1]},
    {action:'delete',index:0},{action:'replace',palette:[0,0,2,1]}]) {
    assert.throws(() => engine.palette(command));assert.deepEqual(engine.snapshot(),before);
  }
  const colors = Float32Array.of(.2,.4,.6,1,0,0,0,0);
  engine.palette({action:'choose',palette:colors});const owned = engine.snapshot();colors.fill(1);
  const runtime = engine.paletteRuntime();runtime.backup.fill(1);
  assert.deepEqual(engine.snapshot(),owned);
  engine.palette({action:'color',index:-1,color:[0,0,0,1]});
  assert.equal(engine.paletteRuntime().customized,true,'native out-of-range color setter still marks customization');
  assert.deepEqual(engine.selectedProfile().settings.palette,owned.profile.settings.palette);
  for (const state of [null,{backup:[],customized:1},{backup:[0,NaN,0,1],customized:false}])
    assert.throws(() => new PaletteSelection({runtime:state}));
});

test('HTTP selected controls and palette runtime survive the real worker and source replacement',async t => {
  const reference = references.find(row => row.id === 'source-load-runtime');
  const first = reference.actions[0][1],second = reference.actions.find(action => action[0] === 'open')[1];
  const host = await startIsolatedGui({port:0});t.after(() => host.close());
  const headers = {'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const post = (action,input) => fetch(host.url+'/api/'+action,{method:'POST',headers,body:JSON.stringify(input)});
  let response = await post('open',{name:first,bytes:reference.inputs[first]});assert.equal(response.status,200);
  let state = await response.json();
  response = await post('automatic',{settings:state.settings});assert.equal(response.status,200);state = await response.json();
  const palette = state.palettes.find(candidate => candidate.count === 4).colors.flat();
  for (const command of [{action:'choose',palette},{action:'add'},{action:'delete',index:0}]) {
    response = await post('palette',command);assert.equal(response.status,200);
    const selected = await response.json();
    assert.deepEqual(Object.keys(selected).sort(),['settings','count','paletteColorCount','basicColorCount',
      'recommendedBackgroundMode','paletteRuntime'].sort(),'selection replies carry no image, scene, candidate or SVG payload');
    state = {...state,...selected};
  }
  assert.equal(state.count,4);assert.equal(state.paletteColorCount,5);
  assert.equal(state.settings.palette.length/4,4);assert.equal(state.paletteRuntime.customized,true);
  const backup = state.paletteRuntime.backup,completed = state.svg;
  response = await post('select',{settings:{advanced:true,complexity:11,smoothness:9}});
  assert.equal(response.status,200);state = {...state,...await response.json()};
  assert.equal(state.settings.complexity,11);assert.equal(state.svg,completed);
  assert.equal(state.committedSettings.advanced,false,'selection does not commit processing settings');
  response = await post('select',{settings:{complexity:0}});assert.equal(response.status,400);
  assert.deepEqual((await response.json()).state,state);
  response = await post('open',{name:second,bytes:reference.inputs[second],settings:state.settings,count:state.count});
  assert.equal(response.status,200);state = await response.json();
  assert.equal(state.committedCount,4);assert.equal(state.committedSettings.complexity,11);
  assert.equal(state.committedSettings.advanced,true);assert.deepEqual(state.committedSettings.palette,[]);
  assert.deepEqual(state.paletteRuntime,{backup,customized:false});
  assert.equal(state.paletteColorCount,2);assert.deepEqual(state.settings.palette,[]);
  const restored = new PaletteSelection({count:state.count,palette:state.settings.palette,
    paletteColorCount:state.paletteColorCount,runtime:state.paletteRuntime});
  assert.deepEqual(restored.runtime(),state.paletteRuntime);
  response = await post('palette',{action:'revert'});assert.equal(response.status,200);state = {...state,...await response.json()};
  assert.deepEqual(state.settings.palette,backup);assert.equal(state.paletteColorCount,4);
  assert.deepEqual(state.committedSettings.palette,[],'reverting selected colors does not commit them');
});
