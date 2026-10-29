mod common;
use base64::Engine;
use openvector::{ai::serialize_ai, dxf::{serialize_dxf, DxfCurveMode}, emf::{serialize_emf, EmfDevice}, eps::serialize_eps, pdf::serialize_pdf, svg::serialize_svg};
use openvector::document_controls::DocumentSettings;
use openvector::image_document::{ImageDocument, DocumentSnapshot};
use openvector::palette::CoreSession;
use openvector::export_options::Layout;
use openvector::vector_file::{write_vector_file, VectorFileInput, TextOutput, BinaryOutput};
use openvector::vector_output::{VectorOutputInput, VectorOutputError};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf, sync::atomic::Ordering};

fn fixture(name: &str) -> Vec<Value> { common::read_ndjson(&format!("{name}.ndjson.gz")) }
fn encoded(bytes: &[u8]) -> String { base64::engine::general_purpose::STANDARD.encode(bytes) }
fn prepare(sources: &Value) -> BTreeMap<String, (ImageDocument, DocumentSnapshot)> {
    let mut settings = Value::Object(DocumentSettings::default().to_json());
    for (key, value) in json!({"category":2,"quality":1,"sensitivity":1,"advanced":true,"antiAlias":false,"complexity":6,"minimumPixels":0,"artifactRejection":2,"clusterColors":false,"smoothness":6,"detectCorners":true,"curveComplexity":6,"smoothingAntiAlias":false,"paletteMode":2,"backgroundMode":1,"background":[1,1,1,1]}).as_object().unwrap() { settings[key] = value.clone(); }
    sources.as_object().unwrap().iter().map(|(name, source)| {
        assert_eq!(source["controls"], json!([2,1,1,1,0,6,0,2,0,6,1,6,0,1,1,1,1,1]));
        let mut document = CoreSession::default().create_document();
        document.set_source(source["width"].as_u64().unwrap() as usize, source["height"].as_u64().unwrap() as usize, &common::b64(source["bgra"].as_str().unwrap()), false, 0.0).unwrap();
        for stage in ["segmentation", "smoothing", "fitting"] { assert_eq!(document.run_task(stage, &settings, None).unwrap().state.status, 0, "{name} {stage}"); }
        let state = document.snapshot(); (name.clone(), (document, state))
    }).collect()
}
fn device(bytes: &[u8]) -> EmfDevice {
    let at = |offset| i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    EmfDevice { pixels: [at(72), at(76)], millimeters: [at(80), at(84)], micrometers: [at(100), at(104)] }
}
fn reference_device() -> EmfDevice {
    let rows = fixture("export-interruption-reference");
    let row = rows.iter().skip(1).find(|row| row["request"]["format"] == "emf" && row["request"]["cancelAt"] == 0).unwrap();
    device(&common::b64(row["expected"]["afterProcessExit"].as_str().unwrap()))
}
fn device_json(device: EmfDevice) -> Value { json!({"pixels":device.pixels,"millimeters":device.millimeters,"micrometers":device.micrometers}) }
fn read_device(value: &Value) -> EmfDevice {
    let pair = |key: &str| [value[key][0].as_i64().unwrap() as i32, value[key][1].as_i64().unwrap() as i32];
    EmfDevice { pixels: pair("pixels"), millimeters: pair("millimeters"), micrometers: pair("micrometers") }
}
fn input<'a>(state: &'a DocumentSnapshot, source: &Value, request: &Value, device: EmfDevice) -> VectorFileInput<'a> {
    let geometry = &state.fitting.as_ref().unwrap().geometry;
    let mut vector = VectorOutputInput::new(geometry, source["width"].as_i64().unwrap(), source["height"].as_i64().unwrap());
    vector.layout = [Layout::Stacked, Layout::Cutouts, Layout::GroupedCutouts][request["layout"].as_u64().unwrap() as usize];
    vector.thin_stroke = request["thinStroke"].as_u64().unwrap() != 0;
    let mut input = VectorFileInput::new(vector);
    input.eps.compact = request["style"] == 0;
    input.eps.creation_date = request["creationDate"].as_str().map(str::to_owned);
    input.palette = &state.segmentation.as_ref().unwrap().palette;
    input.dxf_curve_mode = [DxfCurveMode::Splines, DxfCurveMode::SparseLines, DxfCurveMode::DenseLines][request["curveMode"].as_u64().unwrap_or(0) as usize];
    input.reference_device = Some(device); input
}
struct Capture<'a> { bytes: Vec<u8>, checkpoints: usize, cancel_at: usize, progress: &'a mut dyn FnMut(i32) -> Result<(), String>, cancel: std::sync::Arc<std::sync::atomic::AtomicBool> }
impl Capture<'_> {
    fn checkpoint(&mut self, percent: i32) -> Result<(), VectorOutputError> {
        self.checkpoints += 1; if self.checkpoints == self.cancel_at { self.cancel.store(true, Ordering::Relaxed); }
        (self.progress)(percent).map_err(VectorOutputError)
    }
}
impl TextOutput for Capture<'_> {
    fn write(&mut self, text: &str, _: bool) -> Result<(), VectorOutputError> {
        for character in text.chars() { if character == '\n' { self.bytes.push(b'\r'); } self.bytes.push(character as u8); } Ok(())
    }
    fn progress(&mut self, percent: i32) -> Result<(), VectorOutputError> { self.checkpoint(percent) }
}
impl BinaryOutput for Capture<'_> {
    fn write(&mut self, bytes: &[u8], position: u64) -> Result<(), VectorOutputError> {
        let start = position as usize; self.bytes.resize(self.bytes.len().max(start + bytes.len()), 0); self.bytes[start..start + bytes.len()].copy_from_slice(bytes); Ok(())
    }
    fn progress(&mut self, percent: i32) -> Result<(), VectorOutputError> { self.checkpoint(percent) }
}
fn serialize(format: &str, input: &VectorFileInput<'_>, output: &mut Capture<'_>) -> Result<(), VectorOutputError> {
    match format { "svg" => serialize_svg(&input.vector, output), "pdf" => serialize_pdf(&input.vector, output),
        "eps" => serialize_eps(&input.vector, &input.eps, output), "ai" => serialize_ai(&input.vector, output),
        "dxf" => serialize_dxf(&input.vector, input.palette, input.dxf_curve_mode, output), "emf" => serialize_emf(&input.vector, &input.reference_device.unwrap(), output), _ => panic!("invalid format") }
}
fn compare_bytes(actual: &[u8], expected: &[u8], label: &str) {
    let mismatch = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(mismatch.is_none() && actual.len() == expected.len(), "{label}: byte {:?}, actual {} bytes, expected {} bytes", mismatch, actual.len(), expected.len());
}
#[test]
fn all_660_native_serializer_completion_and_cancellation_states() {
    let rows = fixture("export-interruption-reference"); let sources = &rows[0]["inputs"]; let mut documents = prepare(sources); let device = reference_device();
    let (mut completed, mut cancelled) = (0, 0);
    for row in &rows[1..] {
        let name = row["source"].as_str().unwrap(); let (document, state) = documents.get_mut(name).unwrap();
        let mut request = row["request"].clone(); request["creationDate"] = row["expected"]["validatedCreationDate"].clone();
        let input = input(state, &sources[name], &request, device); let cancel_at = request["cancelAt"].as_u64().unwrap() as usize;
        let mut bytes = Vec::new(); let mut checkpoints = 0; let cancel = document.cancellation_handle();
        let result = document.run_export(|progress| {
            let mut output = Capture { bytes: Vec::new(), checkpoints: 0, cancel_at, progress, cancel };
            let result = serialize(request["format"].as_str().unwrap(), &input, &mut output);
            bytes = output.bytes; checkpoints = output.checkpoints; result.map(|_| 0).map_err(|error| error.0)
        });
        assert_eq!(result.is_err(), cancel_at != 0, "{}", row["id"]);
        if cancel_at != 0 { assert_eq!(checkpoints, cancel_at); cancelled += 1; } else { completed += 1; }
        compare_bytes(&bytes, &common::b64(row["expected"]["afterProcessExit"].as_str().unwrap()), row["id"].as_str().unwrap());
        assert_eq!(document.snapshot().fitting.as_ref().unwrap().geometry, state.fitting.as_ref().unwrap().geometry);
    }
    assert_eq!((completed, cancelled), (390, 270));
}

fn creation_date(encoded: &str) -> Option<String> {
    let bytes = common::b64(encoded); let text = String::from_utf8_lossy(&bytes);
    text.lines().find_map(|line| line.strip_prefix("%%CreationDate: ").map(str::to_owned))
}
fn run_child(sources: &Value, requests: &[Value], errors: bool, access: bool) -> (tempfile::TempDir, Vec<Value>) {
    let directory = tempfile::tempdir().unwrap();
    let config = json!({"sources":sources,"requests":requests,"errors":errors,"access":access,"device":device_json(reference_device())});
    let config_path = directory.path().join("config.json"); std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "file_lifecycle_child", "--nocapture"]).env("OPENVECTOR_FILE_CHILD", directory.path());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::process::CommandExt;
        // Root bypasses readonly bits, unlike the native non-elevated capture.
        // Run the child under an actual unprivileged account, not an exemption.
        if unsafe { libc::geteuid() } == 0 {
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
            command.uid(65534).gid(65534);
        }
    }
    let result = command.output().unwrap();
    assert!(result.status.success(), "child: {}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    let observations: Vec<Value> = serde_json::from_slice(&std::fs::read(directory.path().join("observations.json")).unwrap()).unwrap();
    #[cfg(windows)]
    {
        let mut previous = 0;
        for (request, observed) in requests.iter().zip(&observations) {
            let owned = observed["retainedFiles"].as_array().unwrap().len();
            let raw = observed["handleGrowth"].as_i64().unwrap();
            let other = raw - owned as i64;
            if other != previous {
                eprintln!("{}: process handle growth {raw}, owned output handles {owned}, export-call delta {}, other growth {other} (previous {previous})",
                    request["id"], observed["exportHandleGrowth"]);
                previous = other;
            }
        }
    }
    (directory, observations)
}

#[test]
fn file_lifecycle_child() {
    let Some(directory) = std::env::var_os("OPENVECTOR_FILE_CHILD").map(PathBuf::from) else { return; };
    let config: Value = serde_json::from_slice(&std::fs::read(directory.join("config.json")).unwrap()).unwrap();
    let sources = &config["sources"]; let mut documents = prepare(sources); let device = read_device(&config["device"]);
    if config["errors"] == true {
        for format in ["svg", "pdf", "eps", "ai", "dxf", "emf"] {
            std::fs::create_dir(directory.join(format!("blocked.{format}"))).unwrap();
            let path = directory.join(format!("readonly.{format}")); std::fs::write(&path, b"owned-readonly-export-marker\n").unwrap();
            let mut permissions = std::fs::metadata(&path).unwrap().permissions(); permissions.set_readonly(true); std::fs::set_permissions(path, permissions).unwrap();
        }
    }
    #[cfg(windows)]
    let ownership_directory = directory.canonicalize().unwrap();
    #[cfg(windows)]
    let initial_handles = handle_count();
    let mut observations = Vec::new();
    for request in config["requests"].as_array().unwrap() {
        let name = request["source"].as_str().unwrap(); let (document, state) = documents.get_mut(name).unwrap();
        let input = input(state, &sources[name], request, device); let path = directory.join(request["filename"].as_str().unwrap());
        let cancel = document.cancellation_handle(); let mut checkpoints = 0; let mut io_error = None;
        #[cfg(windows)]
        let before_export_handles = handle_count();
        let result = document.run_export(|progress| {
            let outcome = write_vector_file(&path, &input, &mut |percent| {
                checkpoints += 1; if checkpoints == request["cancelAt"].as_u64().unwrap() { cancel.store(true, Ordering::Relaxed); }
                progress(percent).map_err(VectorOutputError)
            }).map_err(|error| error.0)?;
            io_error = outcome.error.map(|error| format!("{error:?}")); Ok(outcome.status as u32)
        });
        #[cfg(windows)]
        let export_handle_growth = i64::from(handle_count()) - i64::from(before_export_handles);
        #[cfg(windows)]
        let retained_files = owned_files(&ownership_directory);
        let current = document.snapshot();
        let file = match std::fs::read(&path) { Ok(bytes) => json!({"readable":true,"bytes":encoded(&bytes)}), Err(error) => json!({"readable":false,"error":format!("{error:?}")}) };
        let observation = json!({"caught":result.is_err(),"methodResult":result.as_ref().ok().map(|state| state.status),"checkpoints":checkpoints,"ioError":io_error,"file":file,"progress":current.progress,"cancelled":current.cancelled});
        #[cfg(windows)]
        let observation = {
            let mut observation = observation;
            if config["access"] == true { observation["access"] = probe_access(&path, &directory); }
            observation["retainedFiles"] = retained_files.into();
            observation["exportHandleGrowth"] = export_handle_growth.into();
            // Keep the process-wide diagnostic, but assert actual export-file
            // ownership: libtest/CRT handles are not native exporter handles.
            observation["handleGrowth"] = (i64::from(handle_count()) - i64::from(initial_handles)).into();
            observation
        };
        observations.push(observation);
    }
    std::fs::write(directory.join("observations.json"), serde_json::to_vec(&observations).unwrap()).unwrap();
    // The test subprocess then exits normally; libc atexit drains retained text
    // at each handle's own offset, while retained EMF handles write nothing.
}
fn assert_at_unwind(observed: &Value, expected: &Value, blocked_by_emf_sharing: bool, after_exit: Option<&str>, label: &str) {
    if expected["readable"] == true {
        assert_eq!(observed["readable"], true, "{label}"); compare_bytes(&common::b64(observed["bytes"].as_str().unwrap()), &common::b64(expected["bytes"].as_str().unwrap()), label);
    } else if cfg!(windows) || !blocked_by_emf_sharing { assert_eq!(observed["readable"], false, "{label}"); }
    else {
        assert_eq!(expected["errno"], 13, "Native EMF sharing denial: {label}");
        assert_eq!(observed["readable"], true, "Unix has no Windows sharing restriction: {label}");
        if let Some(bytes) = after_exit { compare_bytes(&common::b64(observed["bytes"].as_str().unwrap()), &common::b64(bytes), label); }
    }
}
#[test]
fn files_match_all_660_native_exit_payloads() {
    let rows = fixture("export-interruption-reference");
    let requests: Vec<_> = rows[1..].iter().enumerate().map(|(index, row)| {
        let mut request = row["request"].clone(); request["id"] = row["id"].clone(); request["source"] = row["source"].clone();
        request["filename"] = format!("{index}.{}", request["format"].as_str().unwrap()).into(); request["creationDate"] = row["expected"]["validatedCreationDate"].clone(); request
    }).collect();
    let (directory, observations) = run_child(&rows[0]["inputs"], &requests, false, false);
    #[cfg(windows)]
    let mut retained = BTreeMap::new();
    assert_eq!(observations.len(), 660);
    for ((row, request), observed) in rows[1..].iter().zip(&requests).zip(&observations) {
        let cancel = request["cancelAt"].as_u64().unwrap(); assert_eq!(observed["caught"], cancel != 0, "{}", row["id"]);
        assert_eq!(observed["methodResult"], if cancel == 0 { json!(0) } else { Value::Null }); assert!(observed["ioError"].is_null());
        if cancel != 0 { assert_eq!(observed["checkpoints"], cancel); }
        let expected = common::b64(row["expected"]["afterProcessExit"].as_str().unwrap());
        #[cfg(windows)]
        assert_owned_files(observed, &mut retained, request, usize::from(cancel != 0));
        if cancel == 0 { compare_bytes(&common::b64(observed["file"]["bytes"].as_str().unwrap()), &expected, row["id"].as_str().unwrap()); }
        compare_bytes(&std::fs::read(directory.path().join(request["filename"].as_str().unwrap())).unwrap(), &expected, row["id"].as_str().unwrap());
    }
}
#[test]
fn files_match_all_native_buffer_and_large_path_unwind_states() {
    for (name, count) in [("export-stream-buffer-reference", 30), ("export-large-path-reference", 15)] {
        let rows = fixture(name); assert_eq!(rows.len() - 1, count);
        let requests: Vec<_> = rows[1..].iter().enumerate().map(|(index, row)| {
            let mut request = row["request"].clone(); request["source"] = "grid".into(); request["filename"] = format!("{index}.{}", request["format"].as_str().unwrap()).into();
            request["creationDate"] = creation_date(row["expected"]["afterProcessExit"].as_str().unwrap()).into(); request
        }).collect();
        let (directory, observations) = run_child(&json!({"grid":rows[0]["source"]}), &requests, false, false);
        assert_eq!(observations.len(), requests.len(), "{name}");
        #[cfg(windows)]
        let mut retained = BTreeMap::new();
        for ((row, request), observed) in rows[1..].iter().zip(&requests).zip(&observations) {
            let expected = &row["expected"]["documentMethod"]; let after = row["expected"]["afterProcessExit"].as_str().unwrap();
            assert_at_unwind(&observed["file"], &expected["fileAtUnwind"], request["format"] == "emf", Some(after), row["id"].as_str().unwrap());
            compare_bytes(&std::fs::read(directory.path().join(request["filename"].as_str().unwrap())).unwrap(), &common::b64(after), row["id"].as_str().unwrap());
            assert_eq!(observed["progress"], expected["state"]["progress"]); assert_eq!(observed["cancelled"], expected["state"]["cancelled"] == 1);
            #[cfg(windows)]
            {
                let growth = row["expected"]["handleGrowth"].as_u64().unwrap() as usize;
                assert_eq!(growth, row["expected"]["retainedFiles"].as_array().unwrap().len());
                assert_owned_files(observed, &mut retained, request, growth);
            }
        }
    }
}
#[test]
fn files_match_native_same_target_retry_and_open_failures() {
    let rows = fixture("export-file-state-reference"); assert_eq!(rows.len(), 3);
    for trace in &rows[1..] {
        let requests: Vec<_> = trace["steps"].as_array().unwrap().iter().map(|step| {
            let mut request = step["request"].clone(); request["source"] = "grid".into();
            request["creationDate"] = step["expected"]["fileAtUnwind"]["bytes"].as_str().and_then(creation_date).into(); request
        }).collect();
        let (directory, observations) = run_child(&json!({"grid":rows[0]["source"]}), &requests, trace["id"] == "file-errors", false);
        assert_eq!(observations.len(), requests.len(), "{}", trace["id"]);
        #[cfg(windows)]
        let mut retained = BTreeMap::new();
        for (step, observed) in trace["steps"].as_array().unwrap().iter().zip(&observations) {
            let expected = &step["expected"]; let format = step["request"]["format"].as_str().unwrap();
            assert_eq!(observed["methodResult"], expected["methodResult"], "{}", step["request"]["id"]);
            assert_eq!(observed["caught"], expected["caught"] == 1); assert_eq!(observed["progress"], expected["state"]["progress"]);
            // A missing parent or directory is unreadable on both platforms.
            // Only a retained handle to an existing regular EMF file explains
            // a Windows-only read denial; the fixture records that file kind.
            let filename = step["request"]["filename"].as_str().unwrap();
            let blocked_by_emf_sharing = format == "emf" && trace["finalFiles"][filename]["kind"] == "file";
            assert_at_unwind(&observed["file"], &expected["fileAtUnwind"], blocked_by_emf_sharing, None, step["request"]["id"].as_str().unwrap());
            #[cfg(windows)]
            assert_owned_files(observed, &mut retained, &step["request"], usize::from(step["expected"]["caught"] == 1));
        }
        for (name, expected) in trace["finalFiles"].as_object().unwrap() {
            let path = directory.path().join(name);
            match expected["kind"].as_str().unwrap() {
                "absent" => assert!(!path.exists()), "directory" => assert!(path.is_dir()),
                "file" => {
                    // Without Windows sharing, EMF retries succeed and produce
                    // the native completed baseline, rather than being denied.
                    let expected = if !cfg!(windows) && trace["id"] == "same-target" && name.ends_with(".emf") { &trace["finalFiles"]["baseline.emf"] } else { expected };
                    compare_bytes(&std::fs::read(&path).unwrap(), &common::b64(expected["payload"].as_str().unwrap()), name);
                    assert_eq!(std::fs::metadata(&path).unwrap().permissions().readonly(), expected["readOnly"] == true);
                }
                kind => panic!("unknown file kind {kind}"),
            }
        }
    }
}

#[cfg(windows)]
fn probe_access(path: &std::path::Path, directory: &std::path::Path) -> Value {
    // A missing target parent is intentional in the native file-error corpus.
    // The observer's result belongs to the harness, not that absent directory.
    let output = directory.join("access-observation.json");
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "windows_access_probe_child", "--nocapture"])
        .env_remove("OPENVECTOR_FILE_CHILD").env("OPENVECTOR_ACCESS_PATH", path)
        .env("OPENVECTOR_ACCESS_OUTPUT", &output).output().unwrap();
    assert!(result.status.success(), "access probe for {}: {}\n{}", path.display(), String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap()
}

#[cfg(windows)]
#[test]
fn windows_access_probe_child() {
    use std::os::windows::ffi::OsStrExt;
    let Some(path) = std::env::var_os("OPENVECTOR_ACCESS_PATH") else { return; };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileW(name: *const u16, access: u32, share: u32, security: *const std::ffi::c_void, disposition: u32, flags: u32, template: isize) -> isize;
        fn GetFileSizeEx(handle: isize, size: *mut i64) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    let path: Vec<u16> = path.encode_wide().chain(Some(0)).collect();
    let mut observations = Vec::new();
    for access in [0x80000000u32, 0x40000000, 0x10000, 0x80] {
        let handle = unsafe { CreateFileW(path.as_ptr(), access, 7, std::ptr::null(), 3, 0x80, 0) };
        let (mut size, mut error) = (0i64, 0i32);
        if handle == -1 { error = std::io::Error::last_os_error().raw_os_error().unwrap(); }
        else { unsafe { assert_ne!(GetFileSizeEx(handle, &mut size), 0); assert_ne!(CloseHandle(handle), 0); } }
        observations.push(json!({"desiredAccess":access,"opened":handle != -1,"error":error,"size":size}));
    }
    std::fs::write(std::env::var_os("OPENVECTOR_ACCESS_OUTPUT").unwrap(), serde_json::to_vec(&observations).unwrap()).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_sharing_matches_all_348_native_access_probes() {
    let rows = fixture("export-file-access-reference"); let mut count = 0;
    for group in &rows[1..] {
        let requests: Vec<_> = group["requests"].as_array().unwrap().iter().map(|request| {
            let mut request = request.clone(); request["source"] = "grid".into(); request
        }).collect();
        let (_directory, observations) = run_child(&json!({"grid":rows[0]["source"]}), &requests, group["id"] == "file-errors", true);
        assert_eq!(observations.len(), requests.len(), "{}", group["id"]);
        assert_eq!(observations.len(), group["access"].as_array().unwrap().len(), "{}", group["id"]);
        let mut retained = BTreeMap::new();
        for ((observed, expected), request) in observations.iter().zip(group["access"].as_array().unwrap()).zip(&requests) {
            let label = format!("{} {}", group["id"], request["id"]);
            assert_eq!(observed["access"], expected["opens"], "{label}");
            let filename = request["filename"].as_str().unwrap();
            let expected_files = expected["retained"].as_array().unwrap();
            let previous = retained.get(filename).copied().unwrap_or(0);
            assert!(expected_files.len() >= previous, "{label}");
            assert_owned_files(observed, &mut retained, request, expected_files.len() - previous);
            let actual: Vec<_> = observed["retainedFiles"].as_array().unwrap().iter()
                .filter(|file| file["filename"] == filename)
                .map(|file| json!({"access":file["access"],"size":file["size"]})).collect();
            assert_eq!(actual, *expected_files, "{label}");
            count += expected["opens"].as_array().unwrap().len();
        }
    }
    assert_eq!(count, 348);
}

#[cfg(windows)]
fn handle_count() -> u32 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessHandleCount(process: isize, count: *mut u32) -> i32;
    }
    let mut count = 0;
    assert_ne!(unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) }, 0);
    count
}

#[cfg(windows)]
fn assert_owned_files(observed: &Value, expected: &mut BTreeMap<String, usize>, request: &Value, added: usize) {
    if added != 0 { *expected.entry(request["filename"].as_str().unwrap().to_owned()).or_default() += added; }
    let mut actual = BTreeMap::new();
    for file in observed["retainedFiles"].as_array().unwrap() {
        *actual.entry(file["filename"].as_str().unwrap().to_owned()).or_default() += 1usize;
    }
    assert_eq!(&actual, expected, "{}: process handle growth {}", request["id"], observed["handleGrowth"]);
}

#[cfg(windows)]
fn owned_files(directory: &std::path::Path) -> Vec<Value> {
    use std::{ffi::OsString, mem::size_of, os::windows::ffi::OsStringExt};
    // The native ownership probe uses ProcessHandleInformation (51). Its
    // pointer-sized fields must follow this test executable's architecture,
    // rather than the original x86 capture's seven-u32 entry layout.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Entry {
        handle: usize, handle_count: usize, pointer_count: usize,
        access: u32, object_type: u32, attributes: u32, reserved: u32,
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(process: isize, class: u32, information: *mut std::ffi::c_void, length: u32, returned: *mut u32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetFileType(handle: isize) -> u32;
        fn GetFinalPathNameByHandleW(handle: isize, path: *mut u16, length: u32, flags: u32) -> u32;
        fn GetFileSizeEx(handle: isize, size: *mut i64) -> i32;
    }
    let mut snapshot = vec![0usize; 1024];
    let mut returned = 0;
    loop {
        let status = unsafe { NtQueryInformationProcess(GetCurrentProcess(), 51, snapshot.as_mut_ptr().cast(), (snapshot.len() * size_of::<usize>()) as u32, &mut returned) };
        if status == 0 { break; }
        // STATUS_INFO_LENGTH_MISMATCH: grow the aligned buffer and retry.
        assert_eq!(status as u32, 0xc0000004, "process handle snapshot: {status:#x}");
        snapshot.resize(snapshot.len().max((returned as usize).div_ceil(size_of::<usize>())) * 2, 0);
    }
    assert!(returned as usize <= snapshot.len() * size_of::<usize>());
    assert_eq!(returned as usize, 2 * size_of::<usize>() + snapshot[0] * size_of::<Entry>());
    let mut files = Vec::new();
    let mut path = vec![0u16; 32768];
    for index in 0..snapshot[0] {
        let entry = unsafe { snapshot.as_ptr().add(2).cast::<Entry>().add(index).read() };
        if unsafe { GetFileType(entry.handle as isize) } != 1 { continue; }
        let length = unsafe { GetFinalPathNameByHandleW(entry.handle as isize, path.as_mut_ptr(), path.len() as u32, 0) } as usize;
        assert!(length != 0 && length < path.len(), "handle path: {}", std::io::Error::last_os_error());
        let filename = PathBuf::from(OsString::from_wide(&path[..length]));
        let Ok(filename) = filename.strip_prefix(directory) else { continue; };
        let mut size = 0;
        assert_ne!(unsafe { GetFileSizeEx(entry.handle as isize, &mut size) }, 0);
        // Fixture paths use forward slashes; compare path components rather
        // than changing the real target supplied to CreateFileW.
        let filename = filename.iter().map(|part| part.to_str().unwrap()).collect::<Vec<_>>().join("/");
        files.push(json!({"filename":filename,"access":entry.access,"size":size}));
    }
    files.sort_by(|left, right| left["filename"].as_str().cmp(&right["filename"].as_str()));
    files
}
