import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {SourceHistory,SourceDirectories,formatFileSize} from '../src/source-history.mjs';
import {win32} from 'node:path';
import {decodeImage} from '../src/image-input.mjs';

const [, ...references]=gunzipSync(readFileSync(new URL('./fixtures/source-history-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

for (const reference of references) test('recent-file metadata matches native: '+reference.id,()=>{
  const history=new SourceHistory();
  for (const [index,action] of reference.actions.entries()) {
    const [kind,value]=action,expected=reference.expected[index];
    if (kind==='open' || kind==='open-wide') {
      const bytes=Buffer.from(reference.inputs[value],'base64'),image=decodeImage(bytes);
      history.add({filename:value,width:image.width,height:image.height,bytes:bytes.length});
    } else if (kind==='remove-file')history.remove(value);
    else if (kind==='set') {assert.equal(action[2],'clearMruFiles()');history.clear();}
    else if (kind==='describe-size')assert.equal(formatFileSize(value),expected.sizeDescription.text,'byte-size label at '+index);
    else assert.ok(['snapshot','document'].includes(kind),kind);
    if (expected.sourceHistory) {
      // Compare all visible rows. The native startup's unused QPixmap entries
      // are recorded separately and are not visible recent-file metadata.
      const {capacity,files}=expected.sourceHistory;
      assert.deepEqual(history.snapshot(),{capacity,files},'owned file rows at '+index);
    }
  }
});

test('recent-file snapshots own their arrays and invalid additions preserve history',()=>{
  const history=new SourceHistory();
  history.add({filename:'image.bmp',width:13,height:7,bytes:334});
  const expected=history.snapshot(),previous=history.snapshot();
  previous.files[0].filename='changed';previous.files[0].thumbnailDimensions[0]=99;previous.files.length=0;
  assert.deepEqual(history.snapshot(),expected);
  for (const patch of [{filename:''},{width:0},{height:1.5},{bytes:-1},{bytes:Number.MAX_SAFE_INTEGER+1}]) {
    assert.throws(()=>history.add({filename:'other.bmp',width:17,height:11,bytes:626,...patch}));
    assert.deepEqual(history.snapshot(),expected);
  }
});

test('restored source metadata owns its rows and rejects malformed snapshots',()=>{
  const original=new SourceHistory();
  original.add({filename:'missing naïve.png',width:17,height:11,bytes:626});
  original.add({filename:'thin.bmp',width:1,height:33,bytes:186});
  const snapshot=original.snapshot(),expected=structuredClone(snapshot),restored=SourceHistory.fromSnapshot(snapshot);
  snapshot.files[0].filename='changed';snapshot.files[1].thumbnailDimensions[0]=2;snapshot.files.length=0;
  assert.deepEqual(restored.snapshot(),expected);
  for(const change of [state=>{state.capacity=4;},state=>{state.files.push({...state.files[0]});},
    state=>{state.files[0].description=null;},state=>{state.files[0].thumbnailDimensions=[0,1];},
    state=>{state.files[0].thumbnailDimensions=[31,31];},state=>{state.files[0].filename='';}]) {
    const malformed=structuredClone(expected);change(malformed);assert.throws(()=>SourceHistory.fromSnapshot(malformed));
    assert.deepEqual(restored.snapshot(),expected);
  }
});

const directoryReferences=gunzipSync(readFileSync(new URL('./fixtures/source-directories-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
for(const reference of directoryReferences)test('recent directories match native across restart: '+reference.id,()=>{
  let history=new SourceDirectories();
  for(const [phase,actions] of reference.phases.entries()) {
    if(phase>0)history=SourceDirectories.fromSnapshot(history.snapshot());
    for(const [index,[kind,value]] of actions.entries()) {
      if(kind==='open' || kind==='open-wide')history.add(win32.dirname(value).replaceAll('\\','/'));
      else if(kind==='set')assert.equal(actions[index][2],'clearMruFiles()');
      else assert.ok(['snapshot','persist'].includes(kind),kind);
      const native=reference.expected[phase][index].sourceDirectories;
      if(native)assert.deepEqual(history.snapshot(),native);
      else assert.equal(kind,'set','only the generic slot frame lacks the directory observer');
    }
  }
});

test('directory snapshots own their arrays and reject invalid restore state',()=>{
  const history=new SourceDirectories();history.add('D:/naïve');history.add('D:/NAÏVE');
  const expected=history.snapshot(),snapshot=history.snapshot(),restored=SourceDirectories.fromSnapshot(snapshot);
  snapshot.directories[0]='changed';snapshot.directories.length=0;
  assert.deepEqual(history.snapshot(),expected);assert.deepEqual(restored.snapshot(),expected);
  for(const snapshot of [{capacity:3,directories:[]},{capacity:4,directories:['same','same']},
    {capacity:4,directories:['']},{capacity:4,directories:[null]},{capacity:4,directories:Array(1)},
    {capacity:4,directories:['a','b','c','d','e']}])
    assert.throws(()=>SourceDirectories.fromSnapshot(snapshot));
  assert.throws(()=>history.add(''));assert.deepEqual(history.snapshot(),expected);
  assert.deepEqual(restored.clear(),{capacity:4,directories:[]});assert.deepEqual(history.snapshot(),expected);
});
