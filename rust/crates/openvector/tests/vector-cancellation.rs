mod common;
mod editor_support;
mod cancellation_support;

macro_rules! native_cancellation {
    ($name:ident, $fixture:literal, $cases:literal) => {
        #[test]
        fn $name() { cancellation_support::run_corpus($fixture, $cases, $cases); }
    };
}

native_cancellation!(fitting_cancellation, "fitting-cancellation", 40);
native_cancellation!(fitting_region_progress, "fitting-region-progress", 64);
native_cancellation!(fitting_resmoothing, "fitting-resmoothing", 16);
native_cancellation!(fitting_resegmenting, "fitting-resegmenting", 16);
native_cancellation!(fitting_cold_cancellation, "fitting-cold-cancellation", 40);
native_cancellation!(smoothing_cancellation, "smoothing-cancellation", 388);
native_cancellation!(smoothing_warm_cancellation, "smoothing-warm-cancellation", 16);
