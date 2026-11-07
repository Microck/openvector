use openvector::image_measurement_kernel::{ImageKernel, KernelRegion};

#[test]
fn failed_fragment_coloring_preserves_partial_colors_for_the_next_evaluation() {
    // Authored overlapping geometry exercises the existing fragment-scratch error.
    let regions = (0..13).map(|_| KernelRegion { pixel_count: 1, contour: vec![0, 1, 2, 3],
        parent: -1, color: [1.0, 0.0, 0.0, 1.0] }).collect();
    let mut kernel = ImageKernel::new(4, 4, [0, 0, 0, 255].repeat(16), 4, regions, vec![0.0; 64]);
    let overlap = [0.125, 0.125, 0.875, 0.125, 0.875, 0.875, 0.125, 0.875];
    let outside: Vec<f64> = overlap.iter().map(|x| x + 8.0).collect();
    for _ in 0..2 {
        let error = kernel.energy(&overlap).unwrap_err();
        assert_eq!(error.0, "Image fragment scratch identifies a region outside the prepared model.");
        // Exact unchanged-main result: failed coloring partially changed a pixel.
        // Moving every contour off-image must retain that color, not its old error.
        assert_eq!(kernel.energy(&outside).unwrap().to_bits(), 54.16015625_f64.to_bits());
        assert_eq!(kernel.energy(&outside).unwrap().to_bits(), 54.16015625_f64.to_bits());
    }
}
