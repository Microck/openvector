import test from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdtempSync,rmSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,dirname,resolve,basename} from 'node:path';
import {fileURLToPath} from 'node:url';
import {gunzipSync} from 'node:zlib';
import {SavedAdvancedProfiles,defaultAdvancedProfile,defaultAdvancedProfileName} from '../src/saved-profiles.mjs';
import {defaultSelectedProfile} from '../src/settings-profile.mjs';
import {decodeImage} from '../src/image-input.mjs';
import {exportSvg} from '../src/svg.mjs';
import {createBatchCase,assertBatchResult} from './helpers/native-batch.mjs';

const executable=fileURLToPath(new URL('../bin/openvector.mjs',import.meta.url));
const corpus=name=>gunzipSync(readFileSync(new URL('./fixtures/'+name+'-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [, ...single]=corpus('profile-replay-window'),[, ...batches]=corpus('profile-conversion-batch');
const invoke=(directory,args,input)=>spawnSync(process.execPath,[executable,...args],{cwd:directory,input,timeout:30000});
const success=completed=>{assert.equal(completed.status,0,completed.stderr.toString());assert.equal(completed.stderr.length,0);};
const failure=completed=>{assert.equal(completed.status,1);assert.equal(completed.stdout.length,0);assert.ok(completed.stderr.length);};

function context(t) {
  const directory=mkdtempSync(join(tmpdir(),'openvector-profile-conversion-'));
  t.after(()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(basename(directory).startsWith('openvector-profile-conversion-'));
    rmSync(directory,{recursive:true,force:true});
  });
  return {directory,filename:join(directory,'profiles.json')};
}

const configurations=[['visible-window-default-fast-forward',defaultAdvancedProfileName,()=>defaultAdvancedProfile()],
  ['two-count-seven-fast-forward','two-seven',()=>({...defaultAdvancedProfile(),colorCount:7,
    settings:{...defaultAdvancedProfile().settings,paletteMode:0}})],
  ['fixed-empty-count-fast-forward','empty-count',()=>({...defaultSelectedProfile(),colorCount:7,
    settings:{...defaultSelectedProfile().settings,advanced:true,paletteMode:1}})],
  ['flexible-count-seven-fast-forward','flexible',()=>({...defaultAdvancedProfile(),colorCount:7,
    settings:{...defaultAdvancedProfile().settings,paletteMode:2}})]];

for (const [id,name,profile] of configurations) test('profile vectorize matches native Fast Forward SVG: '+id,async t=>{
  const {directory,filename}=context(t),reference=single.find(row=>row.id===id);assert.ok(reference);
  const source=reference.actions.find(action=>action[0]==='open')[1],encoded=Buffer.from(reference.inputs[source],'base64');
  const input=join(directory,'patchwork.bmp');writeFileSync(input,encoded);
  const profiles=new SavedAdvancedProfiles();profiles.put(name,profile());await profiles.writeFile(filename);
  const completed=invoke(directory,['vectorize',input,'--profile',name,'--profiles-file',filename,'--layout','stacked']);success(completed);
  const expected=reference.expected.findLast(frame=>frame.document).document.state,dimensions=decodeImage(encoded);
  const svg=exportSvg({geometry:expected.fittedGeometry,width:dimensions.width,height:dimensions.height,palette:expected.segmentation.palette});
  assert.deepEqual(completed.stdout,Buffer.from(svg));
  assert.equal(JSON.parse(readFileSync(filename)).profiles[0].name,name,'persisted MRU selection');
});

for (const reference of batches) test('profile batch CLI matches cold native files and log: '+reference.id,async t=>{
  const context=createBatchCase(reference,t),filename=join(context.directory,'profiles.json'),profiles=new SavedAdvancedProfiles();
  let selected=defaultSelectedProfile();
  const slots={'setPaletteMode(int)':'paletteMode','setAdvSegmentationComplexity(int)':'complexity','setAdvContourSmoothness(int)':'smoothness'};
  // Inputs come only from declared native actions, never captured settings,
  // geometry, candidate palettes or output bytes.
  for (const [kind,...values] of reference.before) {
    if (kind==='set') {
      const [receiver,signature,value]=values;assert.equal(receiver,2);
      if(signature==='setAdvNumColors(int)')selected.colorCount=value;
      else {const field=slots[signature];assert.ok(field,signature);selected.settings[field]=value;}
    } else if(kind==='save')profiles.put(values[0],selected);
    else assert.ok(['batch','load'].includes(kind));
  }
  await profiles.writeFile(filename);
  const settings=join(context.directory,'batch.json');
  writeFileSync(settings,JSON.stringify({advanced:false,category:3,quality:3,paletteMode:3,colorCount:0,backgroundMode:1,complexity:1,smoothness:1}));
  const before=readFileSync(settings),began=Date.now();
  const completed=invoke(context.directory,['batch',...context.filenames,'--output-dir',context.outputDirectory,'--format','svg',
    '--settings',settings,'--profile','saved','--profiles-file',filename]);success(completed);
  assertBatchResult(JSON.parse(completed.stdout),context,began,Date.now());
  assert.deepEqual(readFileSync(settings),before);assert.equal(JSON.parse(readFileSync(filename)).profiles[0].name,'saved');
});

test('profile conversion rejects incomplete flags, missing names and invalid source without writing profiles/output',async t=>{
  const {directory,filename}=context(t),profiles=new SavedAdvancedProfiles();profiles.put('saved',defaultAdvancedProfile());await profiles.writeFile(filename);
  const input=join(directory,'invalid.bmp'),output=join(directory,'result.svg');writeFileSync(input,'invalid');
  const bytes=readFileSync(filename);
  for (const args of [['vectorize',input,'--profile','saved'],['vectorize',input,'--profiles-file',filename],
    ['vectorize',input,'--profile','saved','--profiles-file','-'],['batch',input,'--profile','saved'],
    ['batch',input,'--profiles-file',filename],['vectorize',input,'--profile','missing','--profiles-file',filename],
    ['vectorize',input,'--profile','saved','--profiles-file',filename]]) {
    failure(invoke(directory,args));assert.deepEqual(readFileSync(filename),bytes);assert.equal(existsSync(output),false);
  }
});

test('profile vectorize supports exact empty names, stdin and edit plans, and protects the storage file',async t=>{
  const {directory,filename}=context(t),profiles=new SavedAdvancedProfiles();profiles.put('',defaultAdvancedProfile());await profiles.writeFile(filename);
  const reference=single.find(row=>row.id==='visible-window-default-fast-forward'),source=reference.actions[0][1];
  const input=Buffer.from(reference.inputs[source],'base64'),edits=join(directory,'edits.json');writeFileSync(edits,'[]');
  const completed=invoke(directory,['vectorize','-','--filename','patchwork.bmp','--profile','','--profiles-file',filename,'--edits',edits],input);
  success(completed);assert.match(completed.stdout.toString(),/<svg/);assert.equal(JSON.parse(readFileSync(filename)).profiles[0].name,'');
  const before=readFileSync(filename);
  failure(invoke(directory,['vectorize','-','--filename','patchwork.bmp','--profile','','--profiles-file',filename,'--output',filename,'--format','svg','--force'],input));
  assert.deepEqual(readFileSync(filename),before);
});

test('auto-detect saved profile selects without silently running automatic conversion',async t=>{
  const {directory,filename}=context(t),profiles=new SavedAdvancedProfiles(),profile=defaultAdvancedProfile();profile.settings.paletteMode=3;
  profiles.put('auto',profile);await profiles.writeFile(filename);
  const reference=single.find(row=>row.id==='auto-mode-fast-forward'),source=reference.actions[0][1],output=join(directory,'output.svg');
  const completed=invoke(directory,['vectorize','-','--filename','patchwork.bmp','--profile','auto','--profiles-file',filename,'--output',output],
    Buffer.from(reference.inputs[source],'base64'));
  failure(completed);assert.match(completed.stderr.toString(),/Fast Forward does not process/);assert.equal(existsSync(output),false);
  assert.equal(JSON.parse(readFileSync(filename)).profiles[0].name,'auto');
});
