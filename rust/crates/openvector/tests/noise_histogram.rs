use openvector::image_preparation::reduce_prepared_noise;

// Preserve the original sorting implementation as an independent oracle for
// every output byte, including the clamped border's repeated samples.
fn sorted_noise(width: usize, height: usize, bgra: &[u8], amount: u32) -> Vec<u8> {
    let radius = amount.max(1) as i64;
    let mut output = vec![0; bgra.len()];
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            for channel in 0..4 {
                let mut samples = Vec::new();
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        let row = (y + dy).clamp(0, height as i64 - 1) as usize;
                        let column = (x + dx).clamp(0, width as i64 - 1) as usize;
                        samples.push(bgra[(row * width + column) * 4 + channel]);
                    }
                }
                samples.sort_unstable();
                let middle = samples.len() / 2;
                let mut value = samples[middle];
                if value == samples[0] {
                    if let Some(&next) = samples[middle + 1..].iter().find(|&&sample| sample != value) { value = next; }
                } else if value == samples[samples.len() - 1] {
                    value = *samples[..middle].iter().rev().find(|&&sample| sample != value).unwrap();
                }
                output[(y as usize * width + x as usize) * 4 + channel] = value;
            }
        }
    }
    output
}

#[test]
fn rolling_histograms_match_sorted_windows_and_keep_input_owned() {
    let mut seed = 0x2e5a_6c37u32;
    let mut cases = 0;
    for (width, height) in [(5, 5), (5, 8), (7, 7), (7, 11), (15, 13), (32, 17)] {
        for pattern in 0..20 {
            let mut bgra = vec![0; width * height * 4];
            for y in 0..height { for x in 0..width { for channel in 0..4 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                bgra[(y * width + x) * 4 + channel] = match pattern {
                    0 => 0,
                    1 => 255,
                    2 => [0, 1, 128, 255][channel],
                    3 => if (x + y + channel) % 2 == 0 { 0 } else { 255 },
                    4 => ((x * 13 + y * 7 + channel * 59) % 256) as u8,
                    5 => if seed >> 28 == 0 { 255 } else { 0 },
                    6 => if seed >> 28 == 0 { 0 } else { 255 },
                    7 => [0, 15, 16, 127, 128, 239, 240, 255][(seed >> 16) as usize % 8],
                    _ => (seed >> 24) as u8,
                };
            }}}
            let before = bgra.clone();
            // Amounts 0/1 and 4 additionally guard the unchanged fallback.
            for amount in 0..=4 {
                let kernel = amount.max(1) as usize * 2 + 1;
                if width < kernel || height < kernel { continue; }
                let expected = sorted_noise(width, height, &bgra, amount);
                let mut actual = reduce_prepared_noise(width, height, &bgra, amount).unwrap();
                assert_eq!((actual.width, actual.height), (width, height));
                assert_eq!(actual.bgra, expected, "{width}x{height}, pattern {pattern}, amount {amount}");
                actual.bgra.fill(17);
                assert_eq!(bgra, before);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 480);
}

#[test]
fn nonpeak_selection_moves_to_a_distinct_value_at_both_extremes() {
    for amount in [2, 3] {
        let width = amount as usize * 2 + 1;
        let middle = (width * width / 2) * 4;
        for (common, rare) in [(0, 255), (255, 0), (15, 16), (240, 239)] {
            let mut bgra = vec![common; width * width * 4];
            bgra[middle..middle + 4].fill(rare);
            let actual = reduce_prepared_noise(width, width, &bgra, amount).unwrap();
            assert_eq!(&actual.bgra[middle..middle + 4], &[rare; 4]);
            assert_eq!(actual.bgra, sorted_noise(width, width, &bgra, amount));
        }
    }
}
