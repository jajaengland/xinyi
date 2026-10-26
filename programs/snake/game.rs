pub const WIDTH: usize = 24;
pub const HEIGHT: usize = 18;
pub const CELLS: usize = WIDTH * HEIGHT;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction { Up, Down, Left, Right }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step { Moved, Ate, Lost, Won }

pub struct Game {
    // Packed row-major positions; index 0 is the head.
    pub body: [u16; CELLS],
    pub length: usize,
    pub food: Option<u16>,
    pub direction: Direction,
    pending: Direction,
    pub over: bool,
    pub won: bool,
    rng: u32,
}
impl Game {
    pub fn new(seed: u32) -> Self {
        let mut game = Self {
            body: [0;CELLS], length: 3, food: None, direction: Direction::Right,
            pending: Direction::Right, over: false, won: false, rng: seed | 1,
        };
        let head = (HEIGHT/2 * WIDTH + WIDTH/2) as u16;
        game.body[..3].copy_from_slice(&[head,head-1,head-2]);
        // First dot is ahead of the starting snake so the beep is easy to try.
        game.food = Some(head + 4);
        game
    }
    pub fn score(&self) -> u32 { (self.length - 3) as u32 }
    pub fn interval(&self) -> u32 { 20u32.saturating_sub(self.score()/3).max(6) }
    pub fn turn(&mut self, direction: Direction) {
        if self.over {return;}
        let opposite = matches!((self.direction,direction),
            (Direction::Up,Direction::Down)|(Direction::Down,Direction::Up)
            |(Direction::Left,Direction::Right)|(Direction::Right,Direction::Left));
        // Validate against the last actual move, so rapid key presses cannot
        // queue two turns and reverse straight into the neck in one tick.
        if !opposite { self.pending=direction; }
    }
    fn random(&mut self) -> usize {
        self.rng^=self.rng<<13; self.rng^=self.rng>>17; self.rng^=self.rng<<5;
        self.rng as usize
    }
    fn place_food(&mut self) {
        let free=CELLS-self.length;
        if free==0 {self.food=None; self.over=true; self.won=true; return;}
        let mut choice=self.random()%free;
        // Bounded selection over free cells, even when the board is almost full.
        for cell in 0..CELLS as u16 {
            if !self.body[..self.length].contains(&cell) {
                if choice==0 {self.food=Some(cell); return;}
                choice-=1;
            }
        }
    }
    pub fn step(&mut self) -> Step {
        if self.over {return if self.won {Step::Won} else {Step::Lost};}
        self.direction=self.pending;
        let head=self.body[0] as usize;
        let (x,y)=(head%WIDTH,head/WIDTH);
        let next=match self.direction {
            Direction::Up if y>0=>head-WIDTH,
            Direction::Down if y+1<HEIGHT=>head+WIDTH,
            Direction::Left if x>0=>head-1,
            Direction::Right if x+1<WIDTH=>head+1,
            _=>{self.over=true;return Step::Lost;}
        } as u16;
        let eating=self.food==Some(next);
        // The tail vacates its cell on a non-growing move.
        let solid=self.length-if eating {0}else{1};
        if self.body[..solid].contains(&next) {self.over=true;return Step::Lost;}
        if eating {self.length+=1;}
        self.body.copy_within(0..self.length-1,1);
        self.body[0]=next;
        if eating {self.place_food(); if self.won {Step::Won}else{Step::Ate}}
        else {Step::Moved}
    }
    pub fn frame(&self) -> [[u8;WIDTH];HEIGHT] {
        let mut frame=[[0;WIDTH];HEIGHT];
        if let Some(food)=self.food {frame[food as usize/WIDTH][food as usize%WIDTH]=3;}
        for (index,&cell) in self.body[..self.length].iter().enumerate() {
            frame[cell as usize/WIDTH][cell as usize%WIDTH]=if index==0 {2}else{1};
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn eating_grows_and_places_food_on_free_cell() {
        let mut game=Game::new(1);
        for _ in 0..3 {assert_eq!(game.step(),Step::Moved);}
        assert_eq!(game.step(),Step::Ate);
        assert_eq!(game.length,4);assert_eq!(game.score(),1);
        assert!(!game.body[..game.length].contains(&game.food.unwrap()));
    }
    #[test] fn walls_and_reverse_turns() {
        let mut game=Game::new(2);
        game.turn(Direction::Left);
        game.turn(Direction::Up);game.turn(Direction::Left);
        let head=game.body[0];game.step();
        assert_eq!(game.body[0],head-WIDTH as u16);
        for _ in 0..HEIGHT {game.step();}
        assert!(game.over);assert!(!game.won);
    }
    #[test] fn tail_is_free_but_neck_is_solid() {
        let mut game=Game::new(3);
        game.length=4;game.body[..4].copy_from_slice(&[25,26,50,49]);
        game.direction=Direction::Left;game.pending=Direction::Down;game.food=Some(0);
        assert_eq!(game.step(),Step::Moved);assert_eq!(game.body[0],49);
        game.body[..4].copy_from_slice(&[25,49,50,26]);game.pending=Direction::Down;
        assert_eq!(game.step(),Step::Lost);
    }
    #[test] fn filling_board_wins_without_food_loop() {
        let mut game=Game::new(4);
        game.length=CELLS-1;
        for i in 0..CELLS-1 {game.body[i]=i as u16;}
        game.body.swap(0,CELLS-2);
        game.direction=Direction::Right;game.pending=Direction::Right;
        game.food=Some((CELLS-1) as u16);
        assert_eq!(game.step(),Step::Won);assert_eq!(game.length,CELLS);
        assert_eq!(game.food,None);assert!(game.won);
    }
    #[test] fn game_over_freezes_and_restart_resets() {
        let mut game=Game::new(5);game.body[0]=(WIDTH-1) as u16;
        assert_eq!(game.step(),Step::Lost);
        let body=game.body;game.turn(Direction::Down);game.step();assert_eq!(game.body,body);
        let fresh=Game::new(5);assert!(!fresh.over);assert_eq!(fresh.score(),0);
    }
}
