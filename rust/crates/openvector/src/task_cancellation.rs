//! Port of `src/task-cancellation.mjs`: the shared task cancellation signal.
//! Profile selection and document tasks unwind with this error when the
//! controlling task requests cancellation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelTaskException(pub String);

impl std::fmt::Display for CancelTaskException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for CancelTaskException {}
