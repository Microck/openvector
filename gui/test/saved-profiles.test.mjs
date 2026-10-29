import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {SavedAdvancedProfiles,defaultAdvancedProfile} from '../../src/saved-profiles.mjs';
import {quantizeQColorChannels} from '../../src/pixel-input.mjs';
import {compareDocumentState,profileFromFrame} from '../../test/helpers/native-gui.mjs';

const slots = {'setUseAdvancedMode(bool)':'advanced','setPaletteMode(int)':'paletteMode',
  'setAdvSegmentationComplexity(int)':'complexity','setAdvContourSmoothness(int)':'smoothness',
  'setImageCategory(int)':'category','setImageQuality(int)':'quality','setFlattenImageMode(int)':'backgroundMode'};

for (const corpus of ['profile-document','profile-lifecycle']) {
  const [provenance,...references] = gunzipSync(readFileSync(new URL(`../../test/fixtures/${corpus}-reference.ndjson.gz`,import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  for (const reference of references) test('GUI native profile lifetime: '+reference.id,() => {
    const engine = new GuiEngine(),profiles = new SavedAdvancedProfiles();
    let batchMode = false;
    for (let index=0;index<reference.actions.length;index++) {
      const [kind,...values] = reference.actions[index],expected = reference.expected[index];
      if (kind === 'open' || kind === 'automatic') {
        const source = values[0];
        engine.open({bytes:Buffer.from(reference.inputs[source],'base64'),name:source});
        if (kind === 'automatic') engine.automatic({settings:engine.selectedProfile().settings});
      } else if (kind === 'set') {
        const [receiver,signature,value] = values;
        if (receiver === 2) {
          if (signature === 'setAdvNumColors(int)') engine.configureProfile({colorCount:value});
          else {
            const name = slots[signature];assert.ok(name,signature);
            const converted = typeof engine.selectedProfile().settings[name] === 'boolean' ? Boolean(value) : value;
            engine.configureProfile({settings:{[name]:converted}});
          }
        }
      } else if (kind === 'color') engine.configureProfile({settings:{
        background:quantizeQColorChannels(Float32Array.from(values[0]).reverse())}});
      else if (kind === 'save') profiles.put(values[0],engine.selectedProfile());
      else if (kind === 'batch') batchMode = values[0];
      else if (kind === 'load') {
        const selected = profiles.select(values[0]);
        if (selected.profile && !batchMode) engine.applyProfile(selected.profile);
      } else assert.ok(['snapshot','document'].includes(kind),kind);

      if (expected.selected) {
        assert.deepEqual(engine.selectedProfile(),profileFromFrame(expected.selected),'complete selected profile at action '+index);
        assert.deepEqual(profiles.snapshot(),expected.profiles.map(row => ({name:row.name,profile:profileFromFrame(row)})),
          'stored profiles at action '+index);
      }
      if (kind === 'document') {
        const document = expected.document,actual = engine.snapshot();
        compareDocumentState(actual.document,document.state,provenance.parameters);
        assert.equal(actual.committedCount,Buffer.from(document.state.settingsBits,'hex').readInt32LE(8),
          'profile commitment includes the stored advanced count');
        assert.deepEqual(engine.selectedProfile(),profileFromFrame({settingsBits:document.controls.selectedSettingsBits,
          paletteArgbBits:document.selectedPaletteArgbBits}),'selected controls remain distinct from the committed document');
      }
    }
  });
}

test('GUI profile validation is atomic and every returned profile owns its buffers',() => {
  const engine = new GuiEngine(),initial = engine.snapshot();
  for (const patch of [null,[],{settings:null},{settings:[]},{colorCount:NaN},{paletteColorCount:2147483648},
    {settings:{complexity:0}},{settings:{background:[NaN,0,0,1]}}]) {
    assert.throws(() => engine.configureProfile(patch));assert.deepEqual(engine.snapshot(),initial);
  }
  for (const profile of [null,{}, {...defaultAdvancedProfile(),basicColorCount:NaN}]) {
    assert.throws(() => engine.applyProfile(profile));assert.deepEqual(engine.snapshot(),initial);
  }
  const supplied = defaultAdvancedProfile();supplied.settings.palette = Float32Array.of(.2,.4,.6,1);
  const selected = engine.applyProfile(supplied),before = engine.snapshot();
  supplied.settings.palette.fill(0);selected.settings.palette.fill(1);
  const snapshot = engine.snapshot();snapshot.profile.settings.background.fill(1);snapshot.document.settings.palette.fill(1);
  assert.deepEqual(engine.snapshot(),before);
  const changed = engine.configureProfile({settings:{complexity:11},colorCount:7});
  changed.settings.background.fill(1);
  assert.equal(engine.selectedProfile().settings.complexity,11);
  assert.equal(engine.selectedProfile().colorCount,7);
  assert.deepEqual(engine.snapshot().document,before.document,'draft controls alone never commit or process');
});

test('GUI profile selection retains its completed viewer and rejects invalid source replacement',() => {
  const [,reference] = gunzipSync(readFileSync(new URL('../../test/fixtures/profile-document-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const engine = new GuiEngine(),source = reference.actions[0][1];
  engine.open({bytes:Buffer.from(reference.inputs[source],'base64'),name:source});
  const completed = engine.automatic({settings:engine.selectedProfile().settings});
  engine.applyProfile(defaultAdvancedProfile());
  const selected = engine.state(true);
  for (const key of ['svg','preview','segments','stageKeys','segmentationHistory'])
    assert.deepEqual(selected[key],completed[key],key+' is retained until processing');
  const before = engine.snapshot();
  assert.throws(() => engine.open({bytes:Buffer.from('invalid'),name:'invalid.png'}),/Unsupported image/);
  assert.deepEqual(engine.snapshot(),before);
  assert.deepEqual(engine.state(true),selected);
});
