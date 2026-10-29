import { validateBgra } from './pixel-input.mjs';
import { registerImageKernel } from './image-measurement-kernel.mjs';

const F = Math.fround;
const byteScale = F(1 / 255);

function readImageModel(input) {
  if (input === null || typeof input !== 'object') throw new Error('Expected prepared contour image state.');
  const { width, height, bgra, indices, geometry } = input;
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 2 || height < 2 ||
      width * height > 2147483647) throw new Error('Image measurement needs int32 dimensions of at least 2 by 2.');
  validateBgra(bgra);
  if (bgra.length !== width * height * 4) throw new Error('Image dimensions must match the prepared BGRA pixels.');
  if (geometry === null || typeof geometry !== 'object' || !Array.isArray(geometry.nodes) || geometry.nodes.length < 4 ||
      !Array.isArray(geometry.regions) || geometry.regions.length === 0) throw new Error('Image measurement needs shared nodes and region contours.');
  for (const node of geometry.nodes) if (node === null || typeof node !== 'object' ||
      !Number.isFinite(node.x) || !Number.isFinite(node.y)) throw new Error('Image measurement needs finite node coordinates.');
  const regions = geometry.regions.map(region => {
    if (region === null || typeof region !== 'object' || !Number.isInteger(region.pixelCount) || region.pixelCount < 1 ||
        (!Array.isArray(region.contour) && !(region.contour instanceof Int32Array)) || region.contour.length < 3 ||
        !Number.isInteger(region.parent) || region.parent < -1 || region.parent >= geometry.regions.length) {
      throw new Error('Image measurement needs nonempty contours, region pixel counts and nesting parents.');
    }
    validateBgra(region.bgra);
    if (region.bgra.length !== 4) throw new Error('Each image region needs one BGRA byte color.');
    for (const node of region.contour) if (!Number.isInteger(node) || node < 0 || node >= geometry.nodes.length) {
      throw new Error('Image contours must identify existing nodes.');
    }
    return { pixelCount:region.pixelCount, contour:Array.from(region.contour), parent:region.parent,
      color:Float32Array.from(region.bgra, channel => F(channel * byteScale)) };
  });
  if ((!Array.isArray(indices) && !(indices instanceof Int32Array)) || indices.length !== width * height) {
    throw new Error('Image measurement needs one initial region identifier per pixel.');
  }
  const observed = new Int32Array(regions.length), colors = new Float32Array(width * height * 4);
  for (const [pixel, region] of indices.entries()) {
    if (!Number.isInteger(region) || region < 0 || region >= regions.length) throw new Error('Initial image labels must identify existing regions.');
    observed[region]++;
    colors.set(regions[region].color, pixel * 4);
  }
  for (let region = 0; region < regions.length; region++) if (observed[region] !== regions[region].pixelCount) {
    throw new Error('Initial image labels must match region pixel counts.');
  }
  return { width, height, bgra:Uint8Array.from(bgra), nodeCount:geometry.nodes.length, regions, colors,
    flags:new Uint8Array(width * height) };
}

/** Persistent original image-measurement cache for fixed prepared topology. */
export function createContourImageModel(input) {
  const model = readImageModel(input);
  let kernel;
  const handle = {
    evaluate(positions) {
      if ((!Array.isArray(positions) && !(positions instanceof Float64Array)) || positions.length !== model.nodeCount * 2) {
        throw new Error('Image measurement needs two finite double coordinates per node.');
      }
      for (const coordinate of positions) if (!Number.isFinite(coordinate)) {
        throw new Error('Image measurement needs two finite double coordinates per node.');
      }
      const ownedPositions = Float64Array.from(positions);
      kernel.energy(ownedPositions);
      const gradient = new Float64Array(model.nodeCount * 2);
      kernel.addGradient(gradient, ownedPositions);
      return kernel.snapshot(gradient);
    },
  };
  kernel = registerImageKernel(handle, model);
  return handle;
}
