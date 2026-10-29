import test from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtempSync,readFileSync,writeFileSync,readdirSync,rmSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve,dirname,basename} from 'node:path';
import {fileURLToPath} from 'node:url';

const executable = fileURLToPath(new URL('../bin/openvector.mjs',import.meta.url));
function context(t) {
  const parent = resolve(tmpdir()),directory = mkdtempSync(join(parent,'openvector-profile-cli-'));
  t.after(()=>{
    assert.equal(dirname(resolve(directory)),parent);assert.ok(basename(directory).startsWith('openvector-profile-cli-'));
    rmSync(directory,{recursive:true,force:true});
  });
  const filename = join(directory,'profiles.json');
  const run = (args,options = {}) => spawnSync(process.execPath,[executable,'profiles',...args],
    {cwd:directory,encoding:'utf8',timeout:10000,...options});
  return {directory,filename,run};
}

test('profile CLI help is discoverable and reads no files',t => {
  const {directory,run} = context(t);
  for (const args of [['--help'],['save','--file','missing','--bad','--help'],['show','-h']]) {
    const result = run(args);assert.equal(result.status,0,result.stderr);assert.equal(result.stderr,'');
    assert.match(result.stdout,/Usage: openvector profiles/);assert.match(result.stdout,/--force/);
  }
  const help = spawnSync(process.execPath,[executable,'help','profiles'],{encoding:'utf8'});
  assert.equal(help.status,0);assert.match(help.stdout,/Usage: openvector profiles/);
  for (const args of [['help','constructor'],['toString','--help']]) {
    const unknown = spawnSync(process.execPath,[executable,...args],{encoding:'utf8'});
    assert.equal(unknown.status,0,unknown.stderr);assert.match(unknown.stdout,/Usage: openvector/);
  }
  assert.deepEqual(readdirSync(directory),[]);
});

test('profile CLI reads defaults and shows a reusable JSON profile without creating storage',t => {
  const {filename,run} = context(t);
  const listed = run(['list','--file',filename]);assert.equal(listed.status,0,listed.stderr);
  const entries = JSON.parse(listed.stdout);assert.equal(entries.length,1);
  assert.equal(entries[0].name,'Default Advanced Settings');
  const shown = run(['show','--file',filename,'--name',entries[0].name]);assert.equal(shown.status,0,shown.stderr);
  const profile = JSON.parse(shown.stdout);
  assert.equal(profile.colorCount,2);assert.equal(profile.settings.advanced,true);
  assert.deepEqual(profile.settings.background,[0,0,0,0]);assert.deepEqual(profile.settings.palette,[]);
  assert.equal(existsSync(filename),false);
});

test('profile CLI saves from stdin, protects exact-name overwrites and preserves stored bytes on errors',t => {
  const {filename,run} = context(t);
  const shown = run(['show','--file',filename,'--name','Default Advanced Settings']);
  const profile = JSON.parse(shown.stdout);profile.colorCount = 17;profile.settings.complexity = 9;
  const args = ['save','--file',filename,'--name','logo ü','--input','-'];
  const saved = run(args,{input:JSON.stringify(profile)});assert.equal(saved.status,0,saved.stderr);
  const entry = JSON.parse(saved.stdout);assert.equal(entry.name,'logo ü');assert.equal(entry.profile.colorCount,17);
  assert.ok(Array.isArray(entry.profile.settings.background));
  const original = readFileSync(filename);
  const blocked = run(args,{input:JSON.stringify(profile)});
  assert.equal(blocked.status,1);assert.equal(blocked.stdout,'');assert.match(blocked.stderr,/--force/);
  assert.deepEqual(readFileSync(filename),original);
  const invalid = run([...args,'--force'],{input:'{"settings":null}'});
  assert.equal(invalid.status,1);assert.equal(invalid.stdout,'');assert.deepEqual(readFileSync(filename),original);
  profile.colorCount = 24;
  const replaced = run([...args,'--force'],{input:JSON.stringify(profile)});assert.equal(replaced.status,0,replaced.stderr);
  const loaded = run(['show','--file',filename,'--name','logo ü']);assert.equal(JSON.parse(loaded.stdout).colorCount,24);
});

test('profile CLI selects an exact name, preserves native restart behavior and rejects missing names',t => {
  const {filename,run} = context(t);
  const profile = JSON.parse(run(['show','--file',filename,'--name','Default Advanced Settings']).stdout);
  for (const name of ['Logo','logo','']) {
    const saved = run(['save','--file',filename,'--name',name,'--input','-'],{input:JSON.stringify(profile)});
    assert.equal(saved.status,0,saved.stderr);
  }
  const before = JSON.parse(run(['list','--file',filename]).stdout);
  const selected = run(['select','--file',filename,'--name','Logo']);assert.equal(selected.status,0,selected.stderr);
  const result = JSON.parse(selected.stdout);assert.equal(result.index,before.findIndex(row=>row.name==='Logo'));
  assert.equal(JSON.parse(readFileSync(filename)).profiles[0].name,'Logo');
  const restarted = JSON.parse(run(['list','--file',filename]).stdout);
  assert.equal(restarted[0].name,'Default Advanced Settings');
  const bytes = readFileSync(filename);
  for (const command of ['show','select']) {
    const missing = run([command,'--file',filename,'--name','absent']);
    assert.equal(missing.status,1);assert.equal(missing.stdout,'');assert.match(missing.stderr,/not found/i);
    assert.deepEqual(readFileSync(filename),bytes);
  }
});

test('profile CLI rejects invalid command shapes without creating files',t => {
  const {directory,filename,run} = context(t);
  for (const args of [[],['unknown','--file',filename],['list'],['list','--file','-'],
    ['list','--file',filename,'extra'],['list','--file',filename,'--name','extra'],
    ['show','--file',filename],['show','--file',filename,'--name','x','--force'],
    ['select','--file',filename,'--name','x','--input','-'],['save','--file',filename,'--name','x']]) {
    const result = run(args);assert.equal(result.status,1,result.stdout);assert.equal(result.stdout,'');assert.ok(result.stderr.trim());
    assert.doesNotMatch(result.stderr,/\bat .*\.mjs:\d/);
  }
  assert.deepEqual(readdirSync(directory),[]);
});

test('profile CLI reads file input and fails on corrupt storage without repairing it',t => {
  const {filename,directory,run} = context(t);
  const input = join(directory,'input.json');
  writeFileSync(input,run(['show','--file',filename,'--name','Default Advanced Settings']).stdout);
  const saved = run(['save','--file',filename,'--name','from-file','--input',input]);assert.equal(saved.status,0,saved.stderr);
  writeFileSync(filename,'broken');
  const result = run(['list','--file',filename]);assert.equal(result.status,1);assert.equal(result.stdout,'');
  assert.equal(readFileSync(filename,'utf8'),'broken');
});

test('profile CLI rename/delete preserve source values, exact labels, order and bytes on rejected operations',t=>{
  const {filename,run}=context(t);
  const profile=JSON.parse(run(['show','--file',filename,'--name','Default Advanced Settings']).stdout);
  for (const [name,count] of [['old',7],['destination',11],['',13]]) {
    profile.colorCount=count;
    const saved=run(['save','--file',filename,'--name',name,'--input','-'],{input:JSON.stringify(profile)});
    assert.equal(saved.status,0,saved.stderr);
  }
  const rename=['rename','--file',filename,'--name','old','--new-name','destination'],before=readFileSync(filename);
  const rejected=run(rename);assert.equal(rejected.status,1);assert.equal(rejected.stdout,'');assert.match(rejected.stderr,/Confirm replacement/);
  assert.deepEqual(readFileSync(filename),before);
  for (const newName of ['','old']) {
    const noop=run(['rename','--file',filename,'--name','old','--new-name',newName]);assert.equal(noop.status,0,noop.stderr);
    assert.equal(JSON.parse(noop.stdout).changed,false);assert.deepEqual(readFileSync(filename),before);
  }
  const renamed=run([...rename,'--force']);assert.equal(renamed.status,0,renamed.stderr);
  assert.deepEqual(JSON.parse(renamed.stdout),{index:3,changed:true,replacedIndex:2});
  const stored=JSON.parse(readFileSync(filename));assert.deepEqual(stored.profiles.map(row=>row.name),['Default Advanced Settings','','destination']);
  assert.equal(stored.profiles[2].profile.colorCount,7);
  const empty=run(['rename','--file',filename,'--name','','--new-name',' 設定 🖋 ']);assert.equal(empty.status,0,empty.stderr);
  const bytes=readFileSync(filename);
  for (const command of ['rename','delete']) {
    const missing=run([command,'--file',filename,'--name','missing',...(command==='rename'?['--new-name','x']:[])]);
    assert.equal(missing.status,1);assert.equal(missing.stdout,'');assert.match(missing.stderr,/not found/i);assert.deepEqual(readFileSync(filename),bytes);
  }
  const removed=run(['delete','--file',filename,'--name',' 設定 🖋 ']);assert.equal(removed.status,0,removed.stderr);
  assert.deepEqual(JSON.parse(removed.stdout),{index:1});
  const deletedDefault=run(['delete','--file',filename,'--name','Default Advanced Settings']);assert.equal(deletedDefault.status,0,deletedDefault.stderr);
  assert.deepEqual(JSON.parse(readFileSync(filename)).profiles.map(row=>row.name),['destination']);
  assert.equal(JSON.parse(run(['list','--file',filename]).stdout)[0].name,'Default Advanced Settings');
});

test('profile management CLI rejects misplaced flags without creating storage',t=>{
  const {directory,filename,run}=context(t);
  for (const args of [['rename','--name','x'],['rename','--name','x','--new-name','y','--input','-'],
    ['delete','--name','x','--new-name','y'],['delete','--name','x','--force'],['list','--new-name','y']]) {
    const rejected=run([...args,'--file',filename]);assert.equal(rejected.status,1);assert.equal(rejected.stdout,'');
  }
  assert.deepEqual(readdirSync(directory),[]);
});
