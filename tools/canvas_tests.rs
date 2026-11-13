#[path="../src/canvas.rs"]mod canvas;
use canvas::Canvas;
#[test]fn triangle_fill_is_independent_of_winding(){
    let mut first=[0;64];let mut reverse=[0;64];
    Canvas::new(&mut first,8,8,8).unwrap().triangle((1,1),(5,1),(1,5),7,true);
    Canvas::new(&mut reverse,8,8,8).unwrap().triangle((1,5),(5,1),(1,1),7,true);
    assert_eq!(first,reverse);assert_eq!(first.iter().filter(|&&p|p==7).count(),15);
    assert_eq!(first[2*8+2],7);assert_eq!(first[5*8+5],0);
}
#[test]fn outlines_leave_the_interior_empty_and_degenerate_triangles_draw_lines(){
    let mut data=[0;64];let mut c=Canvas::new(&mut data,8,8,8).unwrap();
    c.triangle((1,1),(6,1),(1,6),3,false);
    assert_eq!(data[2*8+2],0);assert_eq!(data[8+1],3);assert_eq!(data[8+6],3);
    let mut data=[0;64];Canvas::new(&mut data,8,8,8).unwrap().triangle((1,1),(3,3),(5,5),9,true);
    assert_eq!(data.iter().filter(|&&p|p==9).count(),5);
}
#[test]fn triangles_and_lines_clip_extreme_coordinates_without_touching_padding(){
    let mut data=[0;80];let mut c=Canvas::new(&mut data,8,8,10).unwrap();
    c.triangle((i32::MIN,i32::MIN),(i32::MAX,i32::MIN),(0,i32::MAX),4,true);
    c.line((i32::MIN,i32::MIN),(i32::MAX,i32::MAX),6);
    for y in 0..8 {assert_eq!(data[y*10+y],6);assert_eq!(&data[y*10+8..y*10+10],&[0,0]);}
}
#[test]fn clipping_preserves_stride_padding(){
    let mut pixels=[0xdead;24];
    let mut c=Canvas::new(&mut pixels,4,4,6).unwrap();
    c.rect(-2,-2,4,4,7);c.pixel(-1,0,9);c.pixel(4,0,9);c.rect(i32::MAX,i32::MAX,i32::MAX,i32::MAX,8);
    assert_eq!(pixels,[7,7,0xdead,0xdead,0xdead,0xdead,7,7,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead,0xdead]);
}
#[test]fn invalid_layouts_are_refused(){
    let mut data=[0;16];assert!(Canvas::new(&mut data,0,1,4).is_none());
    assert!(Canvas::new(&mut data,5,1,4).is_none());assert!(Canvas::new(&mut data,4,5,4).is_none());
    assert!(Canvas::new(&mut data,1,2,usize::MAX).is_none());
}
#[test]fn fonts_clip_and_handle_newlines(){
    let mut data=[0;16*32];let mut font=[0;4096];font[b'A' as usize*16]=0xc0;
    let mut c=Canvas::new(&mut data,16,32,16).unwrap();c.text(0,0,0xffffff,b"A\nA",&font);
    assert_eq!(data[0],0xffffff);assert_eq!(data[1],0xffffff);assert_eq!(data[2],0);
    assert_eq!(data[16*16],0xffffff);assert_eq!(data[16*16+1],0xffffff);
}
#[test]fn raw_console_glyphs_keep_cp437_bits(){
    let mut pixels=[0;128];let mut font=[0;4096];font[219*16..220*16].fill(255);
    Canvas::new(&mut pixels,8,16,8).unwrap().glyph(0,0,7,219,&font);
    assert!(pixels.iter().all(|&p|p==7));
}

#[test]fn image_blits_clip_without_overwriting_stride_or_edges(){let mut data=[9;24];let image=[1,2,3,4];let mut c=Canvas::new(&mut data,4,4,6).unwrap();assert!(c.blit(-1,-1,2,2,&image));assert!(c.blit(100,0,2,2,&image));assert!(c.blit(-100,0,2,2,&image));assert!(!c.blit(0,0,2,2,&[1]));assert_eq!(data[0],4);assert!(data[1..].iter().all(|p|*p==9));}

#[test]
fn damaged_region_matches_full_render_without_touching_surroundings() {
    fn paint(mut canvas: Canvas<'_>) {
        canvas.rect(0, 0, 32, 32, 1);
        canvas.blit(4, 3, 8, 8, &[2; 64]);
        canvas.line((0, 2), (31, 27), 3);
        canvas.triangle((1, 17), (31, 18), (7, 28), 4, true);
        canvas.glyph(4, 4, 5, b'A', &crate::canvas::text::FONT);
        canvas.label(12, 6, 16, 6, b"OK");
    }
    let mut full = [99; 36 * 32];
    let mut partial = full;
    paint(Canvas::new(&mut full, 32, 32, 36).unwrap());
    paint(Canvas::new(&mut partial, 32, 32, 36).unwrap().clipped(7, 8, 17, 15));
    for y in 0..32 {
        for x in 0..36 {
            let i = y * 36 + x;
            let expected = if (7..24).contains(&x) && (8..23).contains(&y) { full[i] } else { 99 };
            assert_eq!(partial[i], expected, "pixel ({x}, {y})");
        }
    }
}
