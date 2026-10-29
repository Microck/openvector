mod common;
mod editor_support;
mod cancellation_support;
#[test]
fn palette_iteration_cancellation(){cancellation_support::run_corpus("palette-iteration-cancellation",480,480)}
#[test]
fn palette_after_fitting_cancellation(){cancellation_support::run_corpus("palette-after-fitting-cancellation",24,24)}
#[test]
fn palette_resize_cancellation(){cancellation_support::run_corpus("palette-resize-cancellation",24,24)}
#[test]
fn palette_after_cancellation(){cancellation_support::run_corpus("palette-after-cancellation",16,32)}
#[test]
fn vector_cached_requests(){cancellation_support::run_corpus("vector-cached-requests",36,24)}
