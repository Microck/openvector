import assert from 'node:assert/strict';
import {mkdtemp,rm} from 'node:fs/promises';
import {dirname,join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {startGui} from '../server.mjs';

/** Cold HTTP fixtures own their stores, including preference writes on close.
 * Persistence/restart tests instead pass explicit stores to the real host. */
export async function startIsolatedGui(options) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-gui-host-'));
  const cleanup=async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-gui-host-')));
    await rm(directory,{recursive:true,force:true});
  };
  try {
    const host=await startGui({...options,profileFilename:join(directory,'profiles.json')});
    return {...host,close:async()=>{try {await host.close();} finally {await cleanup();}}};
  } catch(error) {await cleanup();throw error;}
}
