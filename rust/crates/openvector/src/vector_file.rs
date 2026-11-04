//! Native vector file ownership: text drains, sharing and interrupted handles.
use crate::ai::serialize_ai;
use crate::dxf::{serialize_dxf, DxfCurveMode};
use crate::emf::{serialize_emf, EmfDevice};
use crate::eps::{serialize_eps, EpsOptions};
use crate::pdf::serialize_pdf;
use crate::svg::serialize_svg;
use crate::vector_output::{VectorOutputError, VectorOutputInput};
use std::io;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

/// Compare existing files by identity, following symlinks and recognizing hardlinks.
/// Unix metadata needs no content access, so FIFOs do not block this check and
/// a writable destination need not also grant read permission.
pub fn same_file_identity(target: &Path, source: &Path) -> io::Result<bool> {
    #[cfg(unix)] {
        use std::os::unix::fs::MetadataExt;
        let target = std::fs::metadata(target)?;
        let source = std::fs::metadata(source)?;
        Ok((target.dev(), target.ino()) == (source.dev(), source.ino()))
    }
    #[cfg(not(unix))] { same_file::is_same_file(target, source) }
}

/// Text is Latin-1 before CRT newline expansion. `drain` models native endl,
/// not a general end-of-line policy. Consumer failures stop serialization.
pub trait TextOutput {
    fn write(&mut self, text: &str, drain: bool) -> Result<(), VectorOutputError>;
    fn progress(&mut self, _percent: i32) -> Result<(), VectorOutputError> { Ok(()) }
}
/// EMF writes contain whole buffered records and explicit physical offsets.
pub trait BinaryOutput {
    fn write(&mut self, bytes: &[u8], position: u64) -> Result<(), VectorOutputError>;
    fn progress(&mut self, _percent: i32) -> Result<(), VectorOutputError> { Ok(()) }
}
pub(crate) fn collect_text(serialize: impl FnOnce(&mut dyn TextOutput) -> Result<(), VectorOutputError>) -> Result<String, VectorOutputError> {
    struct Collector(String);
    impl TextOutput for Collector {
        fn write(&mut self, text: &str, _: bool) -> Result<(), VectorOutputError> {
            for part in text.split_inclusive('\n') {
                if let Some(line) = part.strip_suffix('\n') { self.0.push_str(line); self.0.push_str("\r\n"); }
                else { self.0.push_str(part); }
            } Ok(())
        }
    }
    let mut output = Collector(String::new()); serialize(&mut output)?; Ok(output.0)
}

pub struct VectorFileInput<'a> {
    pub vector: VectorOutputInput<'a>,
    pub eps: EpsOptions,
    pub palette: &'a [f32],
    pub dxf_curve_mode: DxfCurveMode,
    pub reference_device: Option<EmfDevice>,
}
impl<'a> VectorFileInput<'a> {
    pub fn new(vector: VectorOutputInput<'a>) -> Self {
        Self { vector, eps: EpsOptions::default(), palette: &[], dxf_curve_mode: DxfCurveMode::default(), reference_device: None }
    }
}
#[derive(Debug)]
pub struct VectorFileResult { pub status: i32, pub error: Option<io::Error> }
#[derive(Clone, Copy, PartialEq, Eq)]
enum Format { Svg, Pdf, Eps, Ai, Dxf, Emf }
impl Format {
    fn from_path(path: &Path) -> Option<Self> {
        let name = path.as_os_str().as_encoded_bytes();
        [(b".svg".as_slice(), Self::Svg), (b".pdf", Self::Pdf), (b".eps", Self::Eps), (b".ai", Self::Ai), (b".dxf", Self::Dxf), (b".emf", Self::Emf)]
            .into_iter().find_map(|(suffix, format)| name.ends_with(suffix).then_some(format))
    }
    fn serialize<T: TextOutput + BinaryOutput>(self, input: &VectorFileInput<'_>, output: &mut T) -> Result<(), VectorOutputError> {
        match self {
            Self::Svg => serialize_svg(&input.vector, output), Self::Pdf => serialize_pdf(&input.vector, output),
            Self::Eps => serialize_eps(&input.vector, &input.eps, output), Self::Ai => serialize_ai(&input.vector, output),
            Self::Dxf => serialize_dxf(&input.vector, input.palette, input.dxf_curve_mode, output),
            Self::Emf => {
                let Some(device) = input.reference_device.as_ref() else {
                    crate::vector_output::read_vector_output(&input.vector, "EMF")?;
                    return Err(VectorOutputError("EMF referenceDevice needs pixels, millimeters and micrometers pairs of positive int32 integers.".into()));
                };
                serialize_emf(&input.vector, device, output)
            }
        }
    }
}

#[cfg(unix)]
mod native {
    use std::{fs::{File, OpenOptions}, io::{self, Write}, os::{fd::IntoRawFd, unix::fs::FileExt}, path::Path};
    pub struct NativeFile(File);
    impl NativeFile {
        pub fn open(path: &Path, _share: u32, overwrite: bool) -> io::Result<Self> { OpenOptions::new().write(true).create(overwrite).truncate(overwrite).create_new(!overwrite).open(path).map(Self) }
        pub fn write(&mut self, bytes: &[u8], position: Option<u64>) -> io::Result<()> {
            if let Some(position) = position { self.0.write_all_at(bytes, position) } else { self.0.write_all(bytes) }
        }
        pub fn close(self) -> io::Result<()> {
            let descriptor = self.0.into_raw_fd();
            if unsafe { libc::close(descriptor) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
        }
    }
}
#[cfg(windows)]
mod native {
    use std::{ffi::c_void, io, os::windows::ffi::OsStrExt, path::Path};
    #[repr(C)]
    struct Overlapped { internal: usize, internal_high: usize, offset: u32, offset_high: u32, event: isize }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileW(name: *const u16, access: u32, share: u32, security: *const c_void, disposition: u32, flags: u32, template: isize) -> isize;
        fn WriteFile(file: isize, data: *const u8, length: u32, written: *mut u32, overlapped: *mut Overlapped) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    pub struct NativeFile(isize);
    impl NativeFile {
        pub fn open(path: &Path, share: u32, overwrite: bool) -> io::Result<Self> {
            let mut path: Vec<u16> = path.as_os_str().encode_wide().collect();
            if path.contains(&0) { return Err(io::Error::new(io::ErrorKind::InvalidInput, "Export filename contains a null character.")); }
            path.push(0);
            // GENERIC_WRITE | FILE_READ_ATTRIBUTES; CREATE_ALWAYS for approved
            // replacement, otherwise CREATE_NEW; normal file.
            let handle = unsafe { CreateFileW(path.as_ptr(), 0x40000080, share, std::ptr::null(), if overwrite { 2 } else { 1 }, 0x80, 0) };
            if handle == -1 { Err(io::Error::last_os_error()) } else { Ok(Self(handle)) }
        }
        pub fn write(&mut self, mut bytes: &[u8], mut position: Option<u64>) -> io::Result<()> {
            while !bytes.is_empty() {
                let mut written = 0;
                let mut at = Overlapped { internal: 0, internal_high: 0, offset: position.unwrap_or(0) as u32, offset_high: (position.unwrap_or(0) >> 32) as u32, event: 0 };
                let target = if position.is_some() { &mut at } else { std::ptr::null_mut() };
                if unsafe { WriteFile(self.0, bytes.as_ptr(), bytes.len().min(0x40000000) as u32, &mut written, target) } == 0 { return Err(io::Error::last_os_error()); }
                if written == 0 { return Err(io::Error::new(io::ErrorKind::WriteZero, "Vector output write made no progress.")); }
                bytes = &bytes[written as usize..]; if let Some(offset) = &mut position { *offset += written as u64; }
            } Ok(())
        }
        pub fn close(mut self) -> io::Result<()> {
            let handle = std::mem::replace(&mut self.0, -1);
            if unsafe { CloseHandle(handle) } != 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
        }
    }
    impl Drop for NativeFile { fn drop(&mut self) { if self.0 != -1 { unsafe { CloseHandle(self.0); } } } }
}
use native::NativeFile;

struct FileStream { file: Option<NativeFile>, pending: [u8; 4096], used: usize, failure: Option<io::Error>, binary: bool }
impl FileStream {
    fn flush(&mut self) {
        if self.used == 0 || self.failure.is_some() { return; }
        let mut bytes = [0; 8192]; let mut count = 0;
        for &byte in &self.pending[..self.used] { if byte == b'\n' { bytes[count] = b'\r'; count += 1; } bytes[count] = byte; count += 1; }
        self.used = 0;
        if let Some(file) = &mut self.file { if let Err(error) = file.write(&bytes[..count], None) { self.failure = Some(error); } }
    }
    fn text(&mut self, text: &str, drain: bool) {
        if self.failure.is_some() || self.file.is_none() { return; }
        for character in text.chars() {
            if self.used == 4096 { self.flush(); }
            if self.failure.is_some() { return; }
            self.pending[self.used] = character as u8; self.used += 1;
        }
        if drain { self.flush(); }
    }
    fn close(mut self) -> Option<io::Error> {
        if !self.binary { self.flush(); }
        if let Some(file) = self.file.take() { if let Err(error) = file.close() { if self.failure.is_none() { self.failure = Some(error); } } }
        self.failure
    }
}
struct FileCallbacks<'a> { stream: &'a mut FileStream, progress: &'a mut dyn FnMut(i32) -> Result<(), VectorOutputError> }
impl TextOutput for FileCallbacks<'_> {
    fn write(&mut self, text: &str, drain: bool) -> Result<(), VectorOutputError> { self.stream.text(text, drain); Ok(()) }
    fn progress(&mut self, percent: i32) -> Result<(), VectorOutputError> { (self.progress)(percent) }
}
impl BinaryOutput for FileCallbacks<'_> {
    fn write(&mut self, bytes: &[u8], position: u64) -> Result<(), VectorOutputError> {
        if self.stream.failure.is_none() { if let Some(file) = &mut self.stream.file {
            if let Err(error) = file.write(bytes, Some(position)) { self.stream.failure = Some(error); }
        } } Ok(())
    }
    fn progress(&mut self, percent: i32) -> Result<(), VectorOutputError> { (self.progress)(percent) }
}
static INTERRUPTED: Mutex<Vec<FileStream>> = Mutex::new(Vec::new());
static EXIT_REGISTERED: LazyLock<Result<(), i32>> = LazyLock::new(|| {
    let result = unsafe { libc::atexit(drain_interrupted) };
    if result == 0 { Ok(()) } else { Err(result) }
});
extern "C" fn drain_interrupted() {
    let mut streams = INTERRUPTED.lock().unwrap_or_else(|poison| poison.into_inner());
    for stream in streams.drain(..) { stream.close(); }
}

/// Overwrite after caller admission. Unknown suffixes return 1; text open and
/// write/close failures return 4. DXF/EMF failed opens still traverse and return
/// 0. Serialization/progress errors escape and retain the physical handle.
pub fn write_vector_file(filename: &Path, input: &VectorFileInput<'_>, progress: &mut dyn FnMut(i32) -> Result<(), VectorOutputError>) -> Result<VectorFileResult, VectorOutputError> {
    write_vector_file_admitted(filename, input, true, progress)
}

/// Write a complete Node API export to a sibling temporary file, then publish
/// it only if the destination does not exist. Cancellation and failures never
/// truncate a destination or expose a partially written final file.
pub fn write_vector_file_safely(filename: &Path, input: &VectorFileInput<'_>, progress: &mut dyn FnMut(i32) -> Result<(), VectorOutputError>) -> Result<VectorFileResult, VectorOutputError> {
    use std::io::{BufWriter, Seek, SeekFrom, Write};
    let Some(format) = Format::from_path(filename) else { return Ok(VectorFileResult { status: 1, error: None }); };
    let destination = if filename.is_absolute() { filename.to_path_buf() } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(filename),
            Err(error) => return Ok(VectorFileResult { status: 4, error: Some(error) }),
        }
    };
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut temporary = match builder.tempfile_in(parent) {
        Ok(file) => file,
        Err(error) => return Ok(VectorFileResult { status: 4, error: Some(error) }),
    };
    struct SafeCallbacks<'a> {
        writer: BufWriter<&'a mut std::fs::File>,
        progress: &'a mut dyn FnMut(i32) -> Result<(), VectorOutputError>,
        failure: Option<io::Error>,
    }
    impl TextOutput for SafeCallbacks<'_> {
        fn write(&mut self, text: &str, _: bool) -> Result<(), VectorOutputError> {
            if self.failure.is_none() {
                let mut bytes = Vec::with_capacity(text.len());
                for character in text.chars() {
                    if character == '\\n' { bytes.push(b'\\r'); }
                    bytes.push(character as u8);
                }
                if let Err(error) = self.writer.write_all(&bytes) { self.failure = Some(error); }
            }
            Ok(())
        }
        fn progress(&mut self, percent: i32) -> Result<(), VectorOutputError> { (self.progress)(percent) }
    }
    impl BinaryOutput for SafeCallbacks<'_> {
        fn write(&mut self, bytes: &[u8], position: u64) -> Result<(), VectorOutputError> {
            if self.failure.is_none() {
                if let Err(error) = self.writer.seek(SeekFrom::Start(position)).and_then(|_| self.writer.write_all(bytes)) {
                    self.failure = Some(error);
                }
            }
            Ok(())
        }
        fn progress(&mut self, percent: i32) -> Result<(), VectorOutputError> { (self.progress)(percent) }
    }
    let mut output = SafeCallbacks { writer: BufWriter::new(temporary.as_file_mut()), progress, failure: None };
    format.serialize(input, &mut output)?;
    if output.failure.is_none() {
        if let Err(error) = output.writer.flush() { output.failure = Some(error); }
    }
    if let Some(error) = output.failure.take() { return Ok(VectorFileResult { status: 4, error: Some(error) }); }
    drop(output);
    match temporary.persist_noclobber(&destination) {
        Ok(_) => Ok(VectorFileResult { status: 0, error: None }),
        Err(error) => Ok(VectorFileResult { status: 4, error: Some(error.error) }),
    }
}

/// Batch admission uses exclusive creation unless preflight explicitly approved replacement.
pub fn write_vector_file_admitted(filename: &Path, input: &VectorFileInput<'_>, overwrite: bool, progress: &mut dyn FnMut(i32) -> Result<(), VectorOutputError>) -> Result<VectorFileResult, VectorOutputError> {
    let Some(format) = Format::from_path(filename) else { return Ok(VectorFileResult { status: 1, error: None }); };
    // Register before opening: there must never be a retained CRT stream whose
    // exit drain could not be installed.
    if EXIT_REGISTERED.is_err() {
        return Err(VectorOutputError("Vector output could not register its interrupted-stream exit drain.".into()));
    }
    let binary = format == Format::Emf;
    let (file, open_error) = match NativeFile::open(filename, if binary { 0 } else { 3 }, overwrite) {
        Ok(file) => (Some(file), None),
        Err(error) if overwrite && (format == Format::Dxf || binary) => (None, Some(error)),
        Err(error) => return Ok(VectorFileResult { status: 4, error: Some(error) }),
    };
    let mut stream = FileStream { file, pending: [0; 4096], used: 0, failure: None, binary };
    let result = format.serialize(input, &mut FileCallbacks { stream: &mut stream, progress });
    if let Err(error) = result {
        if stream.file.is_some() {
            if binary { std::mem::forget(stream); }
            else { INTERRUPTED.lock().unwrap_or_else(|poison| poison.into_inner()).push(stream); }
        }
        return Err(error);
    }
    let error = stream.close();
    Ok(if let Some(open_error) = open_error { VectorFileResult { status: 0, error: Some(open_error) } }
        else { VectorFileResult { status: if error.is_some() { 4 } else { 0 }, error } })
}
