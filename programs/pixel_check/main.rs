#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"]mod xinyi;
fn main()->u32 {
    let w=xinyi::window::open_sized(b"Pixel boundary check",640,544);assert!(w>0);let w=w as u32;
    assert_eq!(xinyi::window::draw(w,&[xinyi::window::Draw::rect(0,0,640,544,0x123456)]),1);
    let colours=[0xff0000,0x00ff00,0x0000ff,0xffffff];
    assert_eq!(xinyi::window::pixels(w,10,10,2,2,&colours),4);
    let request=[10,10,2,2,colours.as_ptr() as u32];
    assert_eq!(xinyi::syscall(104,if w>1 {w-1}else{w+1},request.as_ptr() as u32,20),-9);
    assert_eq!(xinyi::syscall(104,w,0x100000,20),-14);
    assert_eq!(xinyi::syscall(104,w,request.as_ptr() as u32,19),-22);
    for (x,y,width,height,pointer,expected) in [
        (u32::MAX,10,1,1,colours.as_ptr() as u32,-22),
        (0,0,0,1,0,-22),(0,0,640,4,0,-22),
        (0,0,2,1,0x100000,-14),(0,0,2,1,0x403ffffc,-14),
        (0,0,8,1,0xfffffff0,-14),
    ] {
        let r=[x,y,width,height,pointer];assert_eq!(xinyi::syscall(104,w,r.as_ptr() as u32,20),expected);
    }
    let invalid=[0,0xff000000];assert_eq!(xinyi::window::pixels(w,10,10,2,1,&invalid),-22);
    assert_eq!(xinyi::window::pixels(w,639,543,1,1,&[0xabcdef]),1);
    assert_eq!(xinyi::window::present(w),0);
    loop {let mut e=xinyi::window::Event::default();xinyi::window::event(w,&mut e,true);
        if e.kind==xinyi::window::CLOSE || e.kind==xinyi::window::KEY && matches!(e.code as u16,27|81|113) {return 0;}
    }
}
