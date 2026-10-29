//! Port of `src/image-measurement.mjs`: persistent original image-measurement
//! cache for fixed prepared topology.
use crate::contours::Geometry;
use crate::image_measurement_kernel::{ImageKernel, KernelRegion, Measurement};
use crate::pixel_input::{fail, validate_bgra, InputError};

const BYTE_SCALE: f32 = 1.0 / 255.0;

#[derive(Debug, Clone)]
pub struct ContourImageModel { kernel: ImageKernel }

impl ContourImageModel {
    pub fn new(width: f64, height: f64, bgra: &[u8], indices: &[i64], geometry: &Geometry) -> Result<Self, InputError> {
        if !width.is_finite() || !height.is_finite() || width.fract() != 0.0 || height.fract() != 0.0 || width < 2.0 || height < 2.0
            || width * height > 2147483647.0 {
            return fail("Image measurement needs int32 dimensions of at least 2 by 2.");
        }
        let (width, height) = (width as usize, height as usize);
        validate_bgra(bgra)?;
        if bgra.len() != width * height * 4 { return fail("Image dimensions must match the prepared BGRA pixels."); }
        if geometry.nodes.len() < 4 || geometry.regions.is_empty() { return fail("Image measurement needs shared nodes and region contours."); }
        if geometry.nodes.iter().any(|n| !n.x.is_finite() || !n.y.is_finite()) { return fail("Image measurement needs finite node coordinates."); }
        let count = geometry.regions.len() as i32;
        let mut regions = Vec::with_capacity(geometry.regions.len());
        for region in &geometry.regions {
            if region.pixel_count < 1 || region.contour.len() < 3 || region.parent < -1 || region.parent >= count {
                return fail("Image measurement needs nonempty contours, region pixel counts and nesting parents.");
            }
            if region.contour.iter().any(|&n| n >= geometry.nodes.len()) { return fail("Image contours must identify existing nodes."); }
            regions.push(KernelRegion { pixel_count: region.pixel_count, contour: region.contour.clone(), parent: region.parent,
                color: region.bgra.map(|c| c as f32 * BYTE_SCALE) });
        }
        if indices.len() != width * height { return fail("Image measurement needs one initial region identifier per pixel."); }
        let mut observed = vec![0i32; regions.len()];
        let mut colors = vec![0f32; width * height * 4];
        for (pixel, &region) in indices.iter().enumerate() {
            if region < 0 || region >= regions.len() as i64 { return fail("Initial image labels must identify existing regions."); }
            observed[region as usize] += 1;
            colors[pixel * 4..pixel * 4 + 4].copy_from_slice(&regions[region as usize].color);
        }
        if (0..regions.len()).any(|r| observed[r] != regions[r].pixel_count) { return fail("Initial image labels must match region pixel counts."); }
        Ok(ContourImageModel { kernel: ImageKernel::new(width, height, bgra.to_vec(), geometry.nodes.len(), regions, colors) })
    }

    pub fn evaluate(&mut self, positions: &[f64]) -> Result<Measurement, InputError> {
        if positions.len() != self.kernel.node_count * 2 || positions.iter().any(|c| !c.is_finite()) {
            return fail("Image measurement needs two finite double coordinates per node.");
        }
        self.kernel.energy(positions)?;
        let mut gradient = vec![0.0; self.kernel.node_count * 2];
        self.kernel.add_gradient(&mut gradient, positions);
        Ok(self.kernel.snapshot(gradient))
    }

    /// `readImageKernel`: the trusted kernel for a geometry with matching topology.
    pub fn read_kernel(&mut self, geometry: &Geometry) -> Result<&mut ImageKernel, InputError> {
        let k = &self.kernel;
        if k.node_count != geometry.nodes.len() || k.regions.len() != geometry.regions.len() {
            return fail("Image measurement code 1 needs a matching createContourImageModel instance.");
        }
        for (expected, actual) in k.regions.iter().zip(&geometry.regions) {
            if expected.pixel_count != actual.pixel_count || expected.contour != actual.contour {
                return fail("Smoothing geometry must match the image model region contours and pixel counts.");
            }
        }
        Ok(&mut self.kernel)
    }
}
