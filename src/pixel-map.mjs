const isInt32 = value => Number.isInteger(value) && value >= -2147483648 && value <= 2147483647;

/** Owns segment identifiers independently of the palette and source bitmap. */
export class PixelMap {
  #width;
  #height;
  #segments;

  constructor(input) {
    if (input === null || typeof input !== 'object') throw new Error('Expected a pixel map object.');
    const { width, height, segments } = input;
    if (!isInt32(width) || width < 1 || !isInt32(height) || height < 1) {
      throw new Error('Map width and height must be positive int32 integers.');
    }
    if ((!Array.isArray(segments) && !(segments instanceof Int32Array)) ||
        segments.length !== width * height || width * height > 2147483647) {
      throw new Error('Map segments must contain exactly width * height identifiers.');
    }
    if (Array.isArray(segments)) {
      for (const segment of segments) {
        if (!isInt32(segment)) throw new Error('Each segment identifier must be a signed int32 integer.');
      }
    }
    this.#width = width;
    this.#height = height;
    this.#segments = Int32Array.from(segments);
  }

  floodFill(seed, replacement) {
    const segments = this.#segments;
    if (!Number.isInteger(seed) || seed < 0 || seed >= segments.length) {
      throw new Error('Fill seed must be a valid row-major pixel index.');
    }
    if (!isInt32(replacement)) throw new Error('Replacement must be a signed int32 identifier.');
    const original = segments[seed];
    if (original === replacement) throw new Error('Replacement must differ from the seed segment.');

    // Mark on enqueue so a pixel enters the queue at most once. The reference
    // uses scanlines; this direct traversal preserves the same four-neighbor set.
    const queue = new Int32Array(segments.length);
    let head = 0;
    let tail = 1;
    queue[0] = seed;
    segments[seed] = replacement;
    const visit = index => {
      if (segments[index] === original) {
        segments[index] = replacement;
        queue[tail++] = index;
      }
    };
    while (head < tail) {
      const index = queue[head++];
      const column = index % this.#width;
      if (column > 0) visit(index - 1);
      if (column + 1 < this.#width) visit(index + 1);
      if (index >= this.#width) visit(index - this.#width);
      if (index + this.#width < segments.length) visit(index + this.#width);
    }
  }

  toJSON() {
    return { width: this.#width, height: this.#height, segments: Array.from(this.#segments) };
  }
}
