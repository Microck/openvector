import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,rm,readFile,writeFile,mkdir,readdir} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,dirname,basename,resolve} from 'node:path';
import {gunzipSync} from 'node:zlib';
import {SavedAdvancedProfiles,defaultAdvancedProfile,defaultAdvancedProfileName,applyAdvancedProfile} from '../src/saved-profiles.mjs';
import {defaultDocumentSettings} from '../src/document-controls.mjs';
import {quantizeQColorChannels} from '../src/pixel-input.mjs';
import {cleanStartupProfile,createAutomaticComparisonSession,profileFromFrame} from './helpers/native-gui.mjs';

const references = gunzipSync(readFileSync(new URL('./fixtures/saved-profiles-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const slots = {
  'setImageCategory(int)':'category','setImageQuality(int)':'quality','setPaletteMode(int)':'paletteMode',
  'setUseAdvancedMode(bool)':'advanced','setAdvColorSensitivity(int)':'sensitivity','setAdvUsePaletteAa(bool)':'antiAlias',
  'setAdvSegmentationComplexity(int)':'complexity','setAdvMinNumPixels(int)':'minimumPixels',
  'setAdvAntiAliasingRejection(int)':'artifactRejection','setAdvClusterColors(bool)':'clusterColors',
  'setAdvContourSmoothness(int)':'smoothness','setAdvDetectSharpCorners(bool)':'detectCorners',
  'setAdvCurveComplexity(int)':'curveComplexity','setAdvUseContourAa(bool)':'smoothingAntiAlias','setFlattenImageMode(int)':'backgroundMode',
};

async function directory(t) {
  const parent = resolve(tmpdir()),path = await mkdtemp(join(parent,'openvector-profiles-'));
  t.after(async () => {
    assert.equal(dirname(resolve(path)),parent);assert.ok(basename(path).startsWith('openvector-profiles-'));
    await rm(path,{recursive:true,force:true});
  });
  return path;
}

for (const reference of references) test('native saved profiles: '+reference.id,async t => {
  const filename = join(await directory(t),'profiles.json');
  let profiles = new SavedAdvancedProfiles();
  let selected = {settings:structuredClone(cleanStartupProfile.controls),colorCount:2,basicColorCount:0,
    paletteColorCount:0,recommendedBackgroundMode:0};
  let batch = {settings:{...defaultDocumentSettings(),category:3,quality:3,paletteMode:3,backgroundMode:1,
    background:Float32Array.of(0,0,0,1)},colorCount:2,basicColorCount:0,paletteColorCount:3,recommendedBackgroundMode:0};
  for (let phase=0;phase<reference.phases.length;phase++) {
    let batchMode = false;
    if (phase) {
      profiles = await SavedAdvancedProfiles.readFile(filename);
      // MainModel's selected startup preferences are outside this list API.
      // A subsequent single-profile selection supplies every compared field.
      selected = null;
    }
    for (let index=0;index<reference.phases[phase].length;index++) {
      const [kind,...values] = reference.phases[phase][index];
      const expected = reference.expected[phase].frames[index];
      let selectedIndex = -2;
      if (kind === 'set') {
        const [receiver,signature,value] = values,target = receiver === 1 ? batch : selected;
        if (signature === 'setAdvNumColors(int)') target.colorCount = value;
        else if (signature === 'setBatchNumColors(int)') target.basicColorCount = value;
        else {
          const name = slots[signature];assert.ok(name,signature);
          target.settings[name] = typeof target.settings[name] === 'boolean' ? Boolean(value) : value;
        }
      } else if (kind === 'batch') batchMode = values[0];
      else if (kind === 'white') selected.settings.background = Float32Array.of(1,1,1,1);
      else if (kind === 'color') selected.settings.background = quantizeQColorChannels(Float32Array.from(values[0]).reverse());
      else if (kind === 'automatic') {
        const source = values[0],session = createAutomaticComparisonSession(cleanStartupProfile);
        session.load(Buffer.from(reference.inputs[source],'base64'),source);
        const automatic = session.runAutomatic();
        selected = {settings:automatic.controls,colorCount:automatic.selectedColorCount,basicColorCount:0,
          paletteColorCount:automatic.paletteColorCount,recommendedBackgroundMode:automatic.recommendedBackgroundMode};
      } else if (kind === 'save') profiles.put(values[0],selected);
      else if (kind === 'save-many') for (const name of values[0]) profiles.put(name,selected);
      else if (kind === 'load') {
        const selection = profiles.select(values[0]);selectedIndex = selection.index;
        if (selection.profile) {
          if (batchMode) batch = applyAdvancedProfile(batch,selection.profile,{batch:true});
          else selected = applyAdvancedProfile(null,selection.profile);
        }
      } else if (kind === 'persist') await profiles.writeFile(filename);
      else assert.equal(kind,'snapshot');
      if (expected.action !== 28) continue;
      assert.equal(selectedIndex,expected.selectedIndex,'previous selected index');
      const expectedEntries = expected.profiles.map(row => ({name:row.name,profile:profileFromFrame(row)}));
      assert.deepEqual(profiles.snapshot(),expectedEntries,`profile fields, names and order at ${phase}/${index}`);
      if (selected) assert.deepEqual(selected,profileFromFrame(expected.selected),'selected settings projection');
      assert.deepEqual(batch,profileFromFrame(expected.batch),'batch advanced-only projection');
      assert.deepEqual(defaultAdvancedProfile(),profileFromFrame(expected.defaultAdvanced),'built-in profile');
    }
  }
});

test('profile list and selections own all caller buffers',() => {
  const profiles = new SavedAdvancedProfiles(),supplied = defaultAdvancedProfile();
  supplied.settings.palette = Float32Array.of(.1,.2,.3,1);
  profiles.put('owned',supplied);
  const before = profiles.snapshot();supplied.settings.palette.fill(0);supplied.settings.background.fill(1);
  const selected = profiles.select('owned');selected.profile.settings.palette.fill(1);
  const snapshot = profiles.snapshot();snapshot[0].profile.settings.background.fill(1);snapshot[0].name = 'changed';
  assert.deepEqual(profiles.snapshot(),before);
  const missing = profiles.select('absent');assert.deepEqual(missing,{index:-1,profile:null});
  assert.deepEqual(profiles.snapshot(),before);
  profiles.put(defaultAdvancedProfileName,supplied);
  assert.deepEqual(profiles.snapshot()[0].profile,defaultAdvancedProfile());
});

test('profile storage rejects malformed files and invalid controls without changing its list',async t => {
  const path = await directory(t),filename = join(path,'profiles.json');
  assert.deepEqual((await SavedAdvancedProfiles.readFile(filename)).snapshot(),new SavedAdvancedProfiles().snapshot());
  const profiles = new SavedAdvancedProfiles(),before = profiles.snapshot();
  for (const bad of [null,{}, {...defaultAdvancedProfile(),colorCount:NaN},
    {...defaultAdvancedProfile(),settings:{...defaultAdvancedProfile().settings,complexity:0}}]) {
    assert.throws(()=>profiles.put('invalid',bad));assert.deepEqual(profiles.snapshot(),before);
  }
  for (const contents of ['{','null','[]','{}','{"profiles":{},"oldProfiles":[]}',
    '{"profiles":[{"name":"invalid","profile":null}]}']) {
    await writeFile(filename,contents);await assert.rejects(SavedAdvancedProfiles.readFile(filename));
    assert.equal(await readFile(filename,'utf8'),contents);
  }
});

test('profile writes replace complete files and clean only their own temporary file',async t => {
  const path = await directory(t),filename = join(path,'profiles.json'),profiles = new SavedAdvancedProfiles();
  await profiles.writeFile(filename);profiles.put('saved',defaultAdvancedProfile());await profiles.writeFile(filename);
  assert.equal((await SavedAdvancedProfiles.readFile(filename)).size,2);
  const target = join(path,'directory-target');await mkdir(target);await writeFile(join(target,'keep.txt'),'keep');
  await assert.rejects(profiles.writeFile(target));assert.equal(await readFile(join(target,'keep.txt'),'utf8'),'keep');
  assert.deepEqual((await readdir(path)).sort(),['directory-target','profiles.json']);
});

test('profile rename preserves settings and position while confirmed collisions remove only the destination',() => {
  for (const [source,destination,expected] of [['a','c',['b','c']],['c','a',['a','b']]]) {
    const profiles = new SavedAdvancedProfiles();profiles.remove(defaultAdvancedProfileName);
    for (const [name,count] of [['a',3],['b',5],['c',7]]) profiles.put(name,{...defaultAdvancedProfile(),colorCount:count});
    const before = profiles.snapshot(),index = before.findIndex(row=>row.name===source),replacedIndex = before.findIndex(row=>row.name===destination);
    assert.throws(()=>profiles.rename(source,destination),/Confirm replacement/);assert.deepEqual(profiles.snapshot(),before);
    assert.deepEqual(profiles.rename(source,destination,{force:true}),{index,changed:true,replacedIndex});
    assert.deepEqual(profiles.snapshot().map(row=>row.name),expected);
    assert.deepEqual(profiles.snapshot().find(row=>row.name===destination).profile,before[index].profile);
    assert.equal(profiles.snapshot().find(row=>row.name==='b').profile.colorCount,5);
  }
});

test('profile management preserves exact labels and treats missing, empty and unchanged renames as no-ops',() => {
  const profiles = new SavedAdvancedProfiles();
  for (const name of ['', ' ', 'Logo', 'logo', 'é', 'e\u0301']) profiles.put(name,defaultAdvancedProfile());
  const before = profiles.snapshot(),index = before.findIndex(row=>row.name==='Logo');
  for (const newName of ['', 'Logo']) assert.deepEqual(profiles.rename('Logo',newName),{index,changed:false,replacedIndex:-1});
  assert.deepEqual(profiles.rename('missing','renamed'),{index:-1,changed:false,replacedIndex:-1});
  assert.equal(profiles.remove('missing'),-1);assert.deepEqual(profiles.snapshot(),before);
  for (const call of [()=>profiles.rename(null,'x'),()=>profiles.rename('Logo',null),()=>profiles.rename('Logo','x',{force:1}),()=>profiles.remove(null)]) {
    assert.throws(call);assert.deepEqual(profiles.snapshot(),before);
  }
  assert.equal(profiles.rename('','not empty').changed,true);
  assert.equal(profiles.rename('é','é ').changed,true);profiles.remove('e\u0301');
  assert.deepEqual(profiles.snapshot().map(row=>row.name),['é ','logo','Logo',' ','not empty',defaultAdvancedProfileName]);
});

test('built-in profile rename/delete last for the current session and startup restores the fixed entry',async t => {
  const filename=join(await directory(t),'profiles.json'),profiles=new SavedAdvancedProfiles();
  const custom={...defaultAdvancedProfile(),colorCount:17};profiles.put('custom',custom);
  profiles.rename('custom',defaultAdvancedProfileName,{force:true});
  assert.deepEqual(profiles.snapshot(),[{name:defaultAdvancedProfileName,profile:custom}]);
  await profiles.writeFile(filename);
  assert.deepEqual((await SavedAdvancedProfiles.readFile(filename)).snapshot(),new SavedAdvancedProfiles().snapshot());
  profiles.rename(defaultAdvancedProfileName,'renamed default');await profiles.writeFile(filename);
  const restored=await SavedAdvancedProfiles.readFile(filename);
  assert.deepEqual(restored.snapshot().map(row=>row.name),[defaultAdvancedProfileName,'renamed default']);
  assert.equal(restored.snapshot()[1].profile.colorCount,17);
  assert.equal(profiles.remove('renamed default'),0);assert.equal(profiles.size,0);
  await profiles.writeFile(filename);assert.equal((await SavedAdvancedProfiles.readFile(filename)).size,1);
});
