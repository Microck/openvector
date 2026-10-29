import { openSync, writeSync, closeSync } from 'node:fs';
import { createRequire } from 'node:module';
import { serializeSvg } from './svg.mjs';
import { serializePdf } from './pdf.mjs';
import { serializeEps } from './eps.mjs';
import { serializeAi } from './ai.mjs';
import { serializeDxf } from './dxf.mjs';
import { serializeEmf } from './emf.mjs';

// Original share modes. CRT text files keep read/write sharing but deny
// delete; GDI EMF files share nothing. Both are granted FILE_GENERIC_WRITE
// with FILE_READ_ATTRIBUTES, so attribute queries still open them.
const shareReadWrite = 3, shareNone = 0;
const formats = {
  '.svg':{serialize:serializeSvg,share:shareReadWrite}, '.pdf':{serialize:serializePdf,share:shareReadWrite},
  '.eps':{serialize:serializeEps,share:shareReadWrite}, '.ai':{serialize:serializeAi,share:shareReadWrite},
  '.dxf':{serialize:serializeDxf,share:shareReadWrite}, '.emf':{serialize:serializeEmf,share:shareNone,binary:true},
};

// Node's own fs cannot express share modes on Windows, so Windows saves go
// through a small native handle owner. Other platforms have no share modes;
// a plain descriptor is the whole contract there.
const windowsFiles = process.platform === 'win32' ? loadWindowsFiles() : null;
function loadWindowsFiles() {
  if (process.arch !== 'x64') throw new Error(`OpenVector vector export needs the win32-x64 file module; ${process.arch} is not built.`);
  return createRequire(import.meta.url)('./native/file-win32-x64.node');
}

/** Open a truncating output file. write(bytes,position) takes owned bytes;
 * a null position continues at this handle's own file pointer. */
function openOutput(filename, share) {
  if (windowsFiles) {
    const handle = windowsFiles.open(filename, share);
    return { write:(bytes,position) => windowsFiles.write(handle,bytes,position ?? -1), close:() => windowsFiles.close(handle) };
  }
  const descriptor = openSync(filename, 'w');
  return {
    write(bytes, position) {
      let offset = 0;
      while (offset < bytes.length) {
        const written = writeSync(descriptor, bytes, offset, bytes.length - offset, position === null ? null : position + offset);
        if (written === 0) throw new Error('Vector output write made no progress.');
        offset += written;
      }
    },
    close:() => closeSync(descriptor),
  };
}

const interruptedStreams = new Set();

// Native cancellation unwinds past the writer without closing its FILE. Its
// retained raw buffer drains at process exit, at that handle's own offset.
// Keeping the handle matters when another save truncates the same target.
// Interrupted EMF handles are not listed: GDI keeps no CRT buffer to drain,
// and the process exit closes their handles without further writes.
process.once('exit', () => {
  for (const stream of interruptedStreams) stream.close();
});

function textFileStream(file) {
  let pending = '', failure = null;
  const flush = () => {
    if (pending.length === 0 || failure) return;
    const bytes = Buffer.from(pending.replaceAll('\n', '\r\n'), 'latin1');
    pending = '';
    try { file.write(bytes, null); } catch (error) { failure = error; }
  };
  return {
    write(text, drain = false) {
      if (failure) return;
      let offset = 0;
      while (offset < text.length) {
        if (pending.length === 4096) flush();
        if (failure) return;
        const count = Math.min(4096 - pending.length, text.length - offset);
        pending += text.slice(offset, offset + count);
        offset += count;
      }
      if (drain) flush();
    },
    close() {
      flush();
      try { file.close(); } catch (error) { failure ??= error; }
      return failure;
    },
  };
}

// EMF bytes arrive whole-record buffered with explicit offsets; the serializer
// already owns that schedule, so the stream only forwards and records failure.
function emfFileStream(file) {
  let failure = null;
  return {
    write(bytes, position) {
      if (failure) return;
      try { file.write(bytes, position); } catch (error) { failure = error; }
    },
    close() {
      try { file.close(); } catch (error) { failure ??= error; }
      return failure;
    },
  };
}

/** Internal overwrite save used after document/controller admission. The
 * caller owns cancellation; serializer exceptions retain the open handle.
 * This is separate from the CLI's exclusive destination admission policy. */
export function writeVectorFile(filename, input) {
  // Native dispatch compares the final suffix, including the whole filename
  // '.svg'. path.extname would classify that valid batch output as extensionless.
  const extension = Object.keys(formats).find(suffix => filename.endsWith(suffix));
  if (extension === undefined) return {status:1,error:null};
  const format = formats[extension];
  let file;
  try { file = openOutput(filename, format.share); } catch (error) {
    // Original DXF and EMF ignore their failed open and still walk the
    // document, returning success. A progress exception must therefore
    // escape here just as on a real stream.
    if (extension !== '.dxf' && extension !== '.emf') return {status:4,error};
    format.serialize(input, () => {});
    return {status:0,error};
  }
  const stream = format.binary ? emfFileStream(file) : textFileStream(file);
  try { format.serialize(input, stream.write); } catch (error) {
    if (!format.binary) interruptedStreams.add(stream);
    throw error;
  }
  const error = stream.close();
  return {status:error ? 4 : 0,error};
}
