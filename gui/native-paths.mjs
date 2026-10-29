import { execFile } from 'node:child_process';
import { stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

/** Windows preview host boundary. Paths come from the OS picker, never from
 * browser File objects or temporary uploaded copies. All arguments stay out of
 * a shell command string; aborting the host closes its owned picker process. */
async function nativePaths(mode,initialDirectory,signal) {
  if (process.platform !== 'win32') throw new Error('The desktop file picker currently requires Windows.');
  return new Promise((resolvePick,reject) => {
    execFile('powershell.exe',['-NoProfile','-NonInteractive','-STA','-File',
      fileURLToPath(new URL('./native-picker.ps1',import.meta.url)),'-Mode',mode,'-InitialDirectory',initialDirectory],
    {windowsHide:true,encoding:'utf8',signal,maxBuffer:4*1024*1024},(error,stdout) => {
      if (error) {reject(new Error(mode === 'reveal' ? 'Windows could not open the export folder.' : 'Windows could not open the file picker.',{cause:error}));return;}
      try {
        const paths = JSON.parse(stdout.trim());
        if (!Array.isArray(paths) || paths.some(path => typeof path !== 'string')) throw new Error('Invalid picker result.');
        resolvePick(paths);
      } catch (cause) {reject(new Error('Windows returned an unreadable file selection.',{cause}));}
    });
  });
}

export async function pickPaths(mode,initialDirectory,signal) {
  if (!['image','files','folder'].includes(mode)) throw new Error('Choose an image, files or a folder.');
  return nativePaths(mode,initialDirectory,signal);
}

/** EMF headers describe the real host display. The independent writer still
 * accepts explicit device measurements; it never reads the original program. */
export async function readEmfDevice(signal) {
  if(process.platform!=='win32')throw new Error('EMF display measurement currently requires Windows.');
  return new Promise((resolveDevice,reject)=>{
    execFile('powershell.exe',['-NoProfile','-NonInteractive','-File',
      fileURLToPath(new URL('./native-emf-device.ps1',import.meta.url))],
    {windowsHide:true,encoding:'utf8',signal,maxBuffer:64*1024},(error,stdout)=>{
      if(error) {reject(new Error('Windows could not read the EMF display measurements.',{cause:error}));return;}
      try {resolveDevice(JSON.parse(stdout.trim()));}
      catch(cause) {reject(new Error('Windows returned unreadable EMF display measurements.',{cause}));}
    });
  });
}

/** Match Qt's Windows numeric-locale queries without loading Qt or the
 * original application. The parser owns validation of the resulting glyphs. */
export async function readNumericLocale(signal) {
  if(process.platform!=='win32')throw new Error('Bitmap text locale measurement currently requires Windows.');
  return new Promise((resolveLocale,reject)=>{
    execFile('powershell.exe',['-NoProfile','-NonInteractive','-File',
      fileURLToPath(new URL('./native-numeric-locale.ps1',import.meta.url))],
    {windowsHide:true,encoding:'utf8',signal,maxBuffer:64*1024},(error,stdout)=>{
      if(error) {reject(new Error('Windows could not read the numeric locale.',{cause:error}));return;}
      try {
        const values=JSON.parse(stdout.trim()),names=['decimal','group','zero','positive','negative','exponent'];
        if(!values || Object.keys(values).length!==names.length ||
            names.some(name=>!Number.isInteger(values[name]) || values[name]<0 || values[name]>65535))throw new Error('Invalid numeric locale code units.');
        resolveLocale(Object.fromEntries(names.map(name=>[name,String.fromCharCode(values[name])])));
      } catch(cause) {reject(new Error('Windows returned unreadable numeric locale data.',{cause}));}
    });
  });
}

export async function revealFolder(directory,workingDirectory) {
  if (process.platform !== 'win32') throw new Error('Opening folders currently requires Windows.');
  const path = resolve(workingDirectory,directory);
  let folder;
  try {folder = await stat(path);} catch {throw new Error('The folder does not exist.');}
  if (!folder.isDirectory()) throw new Error('Choose a folder, not a file.');
  await nativePaths('reveal',path);
}
