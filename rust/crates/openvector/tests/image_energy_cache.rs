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

#[test]
fn moving_contours_match_a_dense_sum_of_the_current_colors() {
    let (width, height) = (17, 12);
    let pixels = width * height;
    let background = KernelRegion { pixel_count: pixels as i32 - 4, contour: vec![4, 5, 6, 7],
        parent: -1, color: [0.0, 0.0, 0.0, 1.0] };
    let region = KernelRegion { pixel_count: 4, contour: vec![0, 1, 2, 3],
        parent: 0, color: [0.25, 0.5, 0.75, 1.0] };
    let bgra = [0, 0, 0, 255].repeat(pixels);
    // Once summed after 1.0, each tiny squared residual rounds away.
    let mut colors = [2.0_f32.powi(-28), 0.0, 0.0, 1.0].repeat(pixels);
    colors[0] = 1.0;
    let mut kernel = ImageKernel::new(width, height, bgra.clone(), 8, vec![background, region], colors);
    let mut energies = Vec::new();
    for y in [15.0, 1.25, 5.25, 9.25, 15.0] {
        let square = [1.25, y, 2.75, y, 2.75, y + 1.5, 1.25, y + 1.5,
            -4.0, -4.0, 21.0, -4.0, 21.0, 16.0, -4.0, 16.0];
        let energy = kernel.energy(&square).unwrap();
        let colors = kernel.snapshot(Vec::new()).colors;
        let mut residuals = Vec::new();
        for (color, pixel) in colors.chunks_exact(4).zip(bgra.chunks_exact(4)) {
            let [b, g, r, a] = std::array::from_fn(|i| (color[i] - pixel[i] as f32 * (1.0 / 255.0)) as f64);
            residuals.push(((b * b + g * g) + r * r) + a * a);
        }
        let dense = residuals.iter().fold(0.0, |sum, &pixel| pixel + sum);
        if energies.is_empty() {
            let reversed = residuals.iter().rev().fold(0.0, |sum, &pixel| pixel + sum);
            assert_ne!(dense.to_bits(), reversed.to_bits());
        }
        assert_eq!(energy.to_bits(), (dense * 0.5).to_bits());
        energies.push(energy);
    }
    assert!(energies[1] > energies[0], "moving the contour into the image must change pixel energy");
}
