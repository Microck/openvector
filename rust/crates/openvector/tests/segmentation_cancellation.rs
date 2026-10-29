mod common;
mod editor_support;
mod cancellation_support;

#[test]
fn all_native_segmentation_cancellation_and_retry_checkpoints(){
    for(name,cases,cancellations)in [
        ("segmentation-cancellation",441,441),
        ("segmentation-after-fitting-cancellation",22,22),
        ("segmentation-gradient-cancellation",225,225),
        ("segmentation-after-palette-cancellation",1770,1770),
        ("segmentation-reduced-palette-cancellation",8,8),
        ("segmentation-reduced-palette-warm-cancellation",8,8),
        ("segmentation-cached-palette-cancellation",16,32),
    ]{cancellation_support::run_corpus(name,cases,cancellations);}
}
