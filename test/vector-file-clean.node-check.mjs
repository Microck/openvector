import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtempSync,readFileSync,existsSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {writeVectorFile} from '../src/vector-file.mjs';
import {rectangle} from './helpers/authored-rectangle.mjs';

test('vector file writing handles supported and unsupported destinations',t=>{
  const directory=mkdtempSync(join(tmpdir(),'openvector-vector-file-'));
  t.after(()=>rmSync(directory,{recursive:true,force:true}));
  const input=rectangle(),before=structuredClone(input),target=join(directory,'image.svg');
  assert.deepEqual(writeVectorFile(target,input),{status:0,error:null});
  assert.match(readFileSync(target,'utf8'),/<svg[^>]+viewBox="0 0 4 3"/);
  assert.deepEqual(input,before);
  const unknown=join(directory,'image.unknown');
  assert.deepEqual(writeVectorFile(unknown,input),{status:1,error:null});assert.equal(existsSync(unknown),false);
  const failed=writeVectorFile(join(directory,'missing','image.svg'),input);
  assert.equal(failed.status,4);assert.ok(failed.error);
});
