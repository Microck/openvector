use openvector::pixel_input::*;

#[test]
fn dimensions_retain_int32_boundary_without_overflow() {
    assert_eq!(image_size(1,2147483647).unwrap(),2147483647);
    assert_eq!(image_size(46340,46340).unwrap(),2147395600);
    for (w,h) in [(0,1),(1,0),(-1,1),(1,-1),(46341,46341),(2147483648,1),(1,2147483648),(i64::MAX,i64::MAX)] {
        assert_eq!(image_size(w,h).unwrap_err().message(),"Decoded image dimensions must be positive int32 values with an int32 pixel count.");
    }
}
#[test]
fn palette_input_retains_float32_storage_and_exact_validation_messages() {
    assert!(validate_bgra(&[]).is_ok());
    assert_eq!(validate_bgra(&[0,1,2]).unwrap_err().message(),"BGRA pixels must contain four byte channels per pixel.");
    for palette in [vec![],vec![0.0;3]] { assert_eq!(read_palette_colors(&palette).unwrap_err().message(),"Palette must contain four normalized BGRA channels per color."); }
    for invalid in [-0.1,1.1,f64::NAN,f64::INFINITY,f64::NEG_INFINITY] { assert_eq!(read_palette_colors(&[invalid,0.0,0.0,1.0]).unwrap_err().message(),"Palette channels must be finite numbers from 0 to 1."); }
    let values = [-0.0,0.1,1.0/3.0,1.0]; let colors = read_palette_input(&[1,2,3,4],&values).unwrap();
    for (actual,expected) in colors.iter().zip(values) { assert_eq!(actual.to_bits(),(expected as f32).to_bits()); }
    let quantized = quantize_qcolor_channels(&colors);
    for (actual,value) in quantized.iter().zip(colors) { assert_eq!(actual.to_bits(),(((value as f64*65535.0).round()/65535.0) as f32).to_bits()); }
}
#[test]
fn score_input_validates_indices_and_float32_cost_but_retains_original_cost() {
    let bgra = [0;8]; let palette = [0.0,0.0,0.0,1.0,1.0,1.0,1.0,1.0];
    let score = read_palette_score_input(&bgra,&palette,&[0,1],0.1).unwrap(); assert_eq!(score.indices,[0,1]); assert_eq!(score.cost_per_color.to_bits(),0.1f64.to_bits());
    assert_eq!(read_palette_score_input(&bgra,&palette,&[0],0.0).unwrap_err().message(),"Score needs one palette index per BGRA pixel.");
    for indices in [[-1,0],[0,2],[i64::MAX,0]] { assert_eq!(read_palette_score_input(&bgra,&palette,&indices,0.0).unwrap_err().message(),"Score indices must identify existing palette colors."); }
    for cost in [f64::NAN,f64::INFINITY,f64::MAX] { assert_eq!(read_palette_score_input(&bgra,&palette,&[0,1],cost).unwrap_err().message(),"costPerColor must be a finite float32 number."); }
}
#[test]
fn cmyk_normalization_preserves_opacity_and_rounds_all_byte_products() {
    let mut bgra = Vec::with_capacity(256*256*4); let mut black = Vec::with_capacity(256*256);
    for ink in 0u32..256 { for k in 0u32..256 { bgra.extend_from_slice(&[ink as u8,ink as u8,ink as u8,17]); black.push(k as u8); } }
    normalize_cmyk_pixels(&mut bgra,&black);
    for ink in 0u32..256 { for k in 0u32..256 { let pixel = (ink*256+k) as usize; let expected = (((255-ink)*(255-k)+127)/255) as u8; assert_eq!(&bgra[pixel*4..pixel*4+4],&[expected,expected,expected,17]); } }
}
