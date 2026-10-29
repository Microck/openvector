use super::*;
use base64::Engine;
use std::io::Read;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("openvector-save-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
}
impl Drop for Directory { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }

fn settle(app: &mut OpenVectorApp, cx: &mut Context<OpenVectorApp>) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        app.poll(cx);
        if !app.busy && app.pending.is_empty() { break; }
        assert!(Instant::now() < deadline, "worker did not settle: {}", app.status);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!app.error, "{}", app.status);
}

#[gpui::test]
fn main_export_preserves_source_aliases_and_cancelled_existing_output(cx: &mut gpui::TestAppContext) {
    let directory = Directory::new();
    let fixtures = std::env::var_os("OPENVECTOR_FIXTURES").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures"));
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(fixtures.join("auto-update-reference.ndjson.gz")).unwrap()).read_to_string(&mut text).unwrap();
    let reference: Value = serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
    let original = base64::engine::general_purpose::STANDARD.decode(reference["input"]["bytes"].as_str().unwrap()).unwrap();
    // Decoding is content-based: a bitmap can legitimately have a vector suffix.
    let source = directory.0.join("original.svg"); std::fs::write(&source, &original).unwrap();
    let (app, visual) = cx.add_window_view(|_, cx| OpenVectorApp::with_preferences(directory.0.join("gui.json"), cx).unwrap());
    app.update_in(visual, |app, window, cx| {
        app.open(source.clone(), cx); settle(app, cx);
        app.dispatch(Action::Automatic, window, cx); settle(app, cx);
        assert!(app.document.as_ref().unwrap().vectors.is_some());
        let mut aliases = vec![source.clone(), directory.0.join(".").join("original.svg")];
        #[cfg(unix)] {
            let alias = directory.0.join("linked.svg");
            std::os::unix::fs::symlink(&source, &alias).unwrap(); aliases.push(alias);
        }
        let hardlink = directory.0.join("hardlink.svg");
        std::fs::hard_link(&source, &hardlink).unwrap(); aliases.push(hardlink);
        for alias in aliases {
            app.export_to(alias, window, cx);
            assert!(app.error && app.status.contains("Cannot overwrite the source file"));
            assert!(app.dialog.is_none()); assert!(!app.busy); assert!(app.pending.is_empty());
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
        let output = directory.0.join("other.svg");
        let existing = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"3\"/></svg>";
        std::fs::write(&output, existing).unwrap();
        app.export_to(output.clone(), window, cx);
        assert!(matches!(app.dialog, Some(Dialog::Form(Form::Confirm(_, WorkflowAction::Confirm(Command::Export { .. }))))));
        app.dispatch(Action::Dismiss, window, cx);
        assert_eq!(std::fs::read(&output).unwrap(), existing);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(!app.busy); assert!(app.pending.is_empty());
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            // Overwrite requires write permission, not permission to read the
            // existing destination merely to compare its identity.
            std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o200)).unwrap();
            app.export_to(output.clone(), window, cx);
            let Some(Dialog::Form(Form::Confirm(_, action))) = &app.dialog else {
                panic!("writable destination was rejected: {}", app.status);
            };
            let action = action.clone();
            app.workflow_action(action, window, cx); settle(app, cx);
            std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o600)).unwrap();
            let saved = std::fs::read_to_string(&output).unwrap();
            assert!(saved.contains("<svg") && saved.contains("<path"));
            assert_eq!(std::fs::read(&source).unwrap(), original);
        }
    });
}
