#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"] mod xinyi;
mod game;
use game::{Game, WIDTH, HEIGHT};
use xinyi::{key, Cell};
const COLORS: [u8; 8] = [0x07,0x3b,0x6e,0x5d,0x2a,0x4c,0x19,0x6f];

fn number(x: u8, y: u8, mut value: u32) {
    let mut digits = [b'0';10];
    for i in (0..10).rev() { digits[i] += (value % 10) as u8; value /= 10; }
    xinyi::draw_text(x,y,0x0f,&digits);
}
fn render(game: &Game, paused: bool, previous: &mut [[u8;WIDTH];HEIGHT]) {
    let frame = game.frame();
    for y in 0..HEIGHT {
        if frame[y] == previous[y] { continue; }
        let mut cells = [Cell{character:b' ',color:0x07};20];
        for x in 0..WIDTH {
            let value = frame[y][x];
            let (text,color) = if value >= 9 { (*b"::", COLORS[(value-8) as usize] & 15) }
                else if value > 0 { (*b"[]",COLORS[value as usize]) } else { (*b" .",0x08) };
            cells[x*2]=Cell{character:text[0],color};
            cells[x*2+1]=Cell{character:text[1],color};
        }
        xinyi::draw_cells(29,y as u8+2,&cells);
    }
    *previous=frame;
    number(55,3,game.score); number(55,6,game.lines); number(55,9,game.level()+1);
    for y in 0..4 { for x in 0..4 {
        let yes=game::occupied(game.next,0,x,y);
        xinyi::draw_text(55+x as u8*2,13+y as u8,
            if yes { COLORS[game.next as usize+1] } else {0x07}, if yes {b"[]"} else {b"  "});
    }}
    xinyi::draw_text(28,23,0x0f,if game.over {b" GAME OVER - R restarts  "} else if paused {b" PAUSED - P to resume    "} else {b"                        "});
}
fn screen() {
    xinyi::clear_screen(0x07); xinyi::show_cursor(false);
    xinyi::draw_text(29,0,0x0b,b"X I N Y I   T E T R I S");
    for y in 1..23 { xinyi::draw_text(27,y,0x08,b"||"); xinyi::draw_text(49,y,0x08,b"||"); }
    xinyi::draw_text(27,22,0x08,b"+======================+");
    xinyi::draw_text(55,2,0x0b,b"SCORE"); xinyi::draw_text(55,5,0x0b,b"LINES");
    xinyi::draw_text(55,8,0x0b,b"LEVEL"); xinyi::draw_text(55,12,0x0b,b"NEXT");
    xinyi::draw_text(1,4,0x07,b"Left/Right: move");
    xinyi::draw_text(1,6,0x07,b"Up / X: rotate");
    xinyi::draw_text(1,8,0x07,b"Z: turn back");
    xinyi::draw_text(1,10,0x07,b"Down: soft drop");
    xinyi::draw_text(1,12,0x07,b"Space: hard drop");
    xinyi::draw_text(1,14,0x07,b"P: pause/resume");
    xinyi::draw_text(1,16,0x07,b"R: restart");
    xinyi::draw_text(1,18,0x07,b"Escape: quit");
    xinyi::draw_text(55,19,0x08,b":: landing guide");
}
fn main() -> u32 {
    let mut game=Game::new(xinyi::ticks());
    let mut previous=[[255;WIDTH];HEIGHT];
    let mut paused=false;
    let mut last=xinyi::ticks();
    let mut dirty=true;
    screen();
    loop {
        for _ in 0..32 {
            let event=xinyi::read_key(false);
            if event<=0 { break; }
            let event=event as u32;
            let code=event as u16;
            if code==27 { return 0; }
            if event & (key::CTRL|key::ALT) != 0 { continue; }
            if code==b'p' as u16 || code==b'P' as u16 {
                paused=!paused; last=xinyi::ticks(); dirty=true; continue;
            }
            if code==b'r' as u16 || code==b'R' as u16 {
                game=Game::new(xinyi::ticks()); paused=false; last=xinyi::ticks(); dirty=true; continue;
            }
            if paused || game.over { continue; }
            match code {
                key::LEFT => game.shift(-1), key::RIGHT => game.shift(1),
                key::UP | 120 | 88 => game.rotate(true),
                122 | 90 => game.rotate(false),
                key::DOWN => { if game.down(true) {last=xinyi::ticks();} }
                32 => {game.hard_drop(); last=xinyi::ticks();}
                _=>continue,
            }
            dirty=true;
        }
        let now=xinyi::ticks();
        if !paused && !game.over && now.wrapping_sub(last)>=game.interval() {
            game.down(false); last=now; dirty=true;
        }
        if dirty {render(&game,paused,&mut previous); dirty=false;}
        xinyi::sleep_ms(10);
    }
}
