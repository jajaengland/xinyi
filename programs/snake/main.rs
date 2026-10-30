#![no_std]
#![no_main]
#[path="../../sdk/xinyi.rs"] mod xinyi;
mod game;
use game::{Game,Direction,Step,WIDTH,HEIGHT};
use xinyi::{key,Cell};

fn screen() {
    xinyi::clear_screen(0x07);xinyi::show_cursor(false);
    xinyi::draw_text(29,0,0x0a,b"X I N Y I   S N A K E");
    xinyi::draw_text(15,1,0x08,b"+================================================+");
    xinyi::draw_text(15,20,0x08,b"+================================================+");
    for y in 2..20 {xinyi::draw_text(15,y,0x08,b"|");xinyi::draw_text(64,y,0x08,b"|");}
    xinyi::draw_text(16,22,0x07,b"Arrows/WASD move   P pause   R restart   Esc quit");
    xinyi::draw_text(16,24,0x08,b"Eat dots to grow. Each dot makes a short beep.");
}
fn render(game:&Game,paused:bool,previous:&mut [[u8;WIDTH];HEIGHT]) {
    let frame=game.frame();
    for y in 0..HEIGHT {
        if frame[y]==previous[y] {continue;}
        let mut cells=[Cell{character:b' ',color:0x07};WIDTH*2];
        for x in 0..WIDTH {
            let (text,color)=match frame[y][x] {1=>(*b"[]",0x2a),2=>(*b"[]",0x2e),3=>(*b"()",0x0c),_=>(*b"  ",0x07)};
            cells[x*2]=Cell{character:text[0],color};cells[x*2+1]=Cell{character:text[1],color};
        }
        xinyi::draw_cells(16,y as u8+2,&cells);
    }
    *previous=frame;
    let mut score=game.score();let mut text=*b"DOTS: 000   SPEED: 00";
    for index in (6..9).rev() {text[index]+= (score%10) as u8;score/=10;}
    let speed=21-game.interval();text[19]+=(speed/10) as u8;text[20]+=(speed%10) as u8;
    xinyi::draw_text(16,21,0x0f,&text);
    xinyi::draw_text(40,21,0x0f,if game.won {b"YOU WIN! R restarts    "} else if game.over {b"GAME OVER - R restarts "} else if paused {b"PAUSED - P resumes    "} else {b"                      "});
}
fn main()->u32 {
    let mut game=Game::new(xinyi::ticks());let mut previous=[[255;WIDTH];HEIGHT];
    let mut last=xinyi::ticks();let mut dirty=true;let mut paused=false;
    screen();
    loop {
        for _ in 0..32 {
            let event=xinyi::read_key(false);if event<=0 {break;}
            let event=event as u32;let code=event as u16;
            if code==27 {return 0;}
            if event&(key::CTRL|key::ALT)!=0 {continue;}
            match code {
                112|80=>{paused=!paused;last=xinyi::ticks();dirty=true;}
                114|82=>{game=Game::new(xinyi::ticks());paused=false;last=xinyi::ticks();dirty=true;}
                _ if paused||game.over=>(),
                key::UP|119|87=>game.turn(Direction::Up),
                key::DOWN|115|83=>game.turn(Direction::Down),
                key::LEFT|97|65=>game.turn(Direction::Left),
                key::RIGHT|100|68=>game.turn(Direction::Right),
                _=>(),
            }
        }
        let now=xinyi::ticks();
        if !paused&&!game.over&&now.wrapping_sub(last)>=game.interval() {
            let result=game.step();
            if matches!(result,Step::Ate|Step::Won) {xinyi::beep(880,70);}
            last=xinyi::ticks();dirty=true;
        }
        if dirty {render(&game,paused,&mut previous);dirty=false;}
        xinyi::sleep_ms(10);
    }
}
