pub const WIDTH: usize = 10;
pub const HEIGHT: usize = 20;
// Row-major 4x4 bitmaps. Rotations use a 3x3 box, except I (4x4) and O.
const SHAPES: [u16; 7] = [0x00f0, 0x0066, 0x0072, 0x0036, 0x0063, 0x0071, 0x0074];

pub fn occupied(kind: u8, rotation: u8, x: i32, y: i32) -> bool {
    let size = if kind == 0 { 4 } else { 3 };
    let (mut px, mut py) = (x, y);
    if kind != 1 {
        for _ in 0..rotation % 4 { (px, py) = (py, size - 1 - px); }
    }
    px >= 0 && px < 4 && py >= 0 && py < 4 && SHAPES[kind as usize] & (1 << (py * 4 + px)) != 0
}

pub struct Game {
    pub board: [[u8; WIDTH]; HEIGHT],
    pub kind: u8,
    pub rotation: u8,
    pub x: i32,
    pub y: i32,
    pub next: u8,
    pub score: u32,
    pub lines: u32,
    pub over: bool,
    rng: u32,
    bag: [u8; 7],
    remaining: usize,
}
impl Game {
    pub fn new(seed: u32) -> Self {
        let mut game = Self {
            board: [[0; WIDTH]; HEIGHT], kind: 0, rotation: 0, x: 3, y: 0,
            next: 0, score: 0, lines: 0, over: false, rng: seed | 1,
            bag: [0, 1, 2, 3, 4, 5, 6], remaining: 0,
        };
        game.kind = game.take_piece();
        game.next = game.take_piece();
        game
    }
    fn random(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }
    fn take_piece(&mut self) -> u8 {
        if self.remaining == 0 {
            self.bag = [0, 1, 2, 3, 4, 5, 6];
            for i in (1..7).rev() {
                let j = self.random() as usize % (i + 1);
                self.bag.swap(i, j);
            }
            self.remaining = 7;
        }
        self.remaining -= 1;
        self.bag[self.remaining]
    }
    pub fn level(&self) -> u32 { self.lines / 10 }
    pub fn interval(&self) -> u32 { 70u32.saturating_sub(self.level().saturating_mul(5)).max(8) }
    pub fn fits(&self, x: i32, y: i32, rotation: u8) -> bool {
        for py in 0..4 { for px in 0..4 {
            if occupied(self.kind, rotation, px, py) {
                let (bx, by) = (x + px, y + py);
                if bx < 0 || bx >= WIDTH as i32 || by < 0 || by >= HEIGHT as i32
                    || self.board[by as usize][bx as usize] != 0 { return false; }
            }
        }}
        true
    }
    pub fn shift(&mut self, dx: i32) {
        if !self.over && self.fits(self.x + dx, self.y, self.rotation) { self.x += dx; }
    }
    pub fn rotate(&mut self, clockwise: bool) {
        if self.over { return; }
        let rotation = (self.rotation + if clockwise { 1 } else { 3 }) % 4;
        for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (-2, 0), (2, 0), (0, -1)] {
            if self.fits(self.x + dx, self.y + dy, rotation) {
                self.x += dx; self.y += dy; self.rotation = rotation; return;
            }
        }
    }
    /// Return true when the piece locks and a new one spawns.
    pub fn down(&mut self, manual: bool) -> bool {
        if self.over { return false; }
        if self.fits(self.x, self.y + 1, self.rotation) {
            self.y += 1;
            if manual { self.score = self.score.saturating_add(1); }
            false
        } else { self.lock(); true }
    }
    pub fn landing(&self) -> i32 {
        let mut y = self.y;
        while self.fits(self.x, y + 1, self.rotation) { y += 1; }
        y
    }
    pub fn hard_drop(&mut self) {
        if self.over { return; }
        let y = self.landing();
        self.score = self.score.saturating_add((y - self.y) as u32 * 2);
        self.y = y;
        self.lock();
    }
    fn lock(&mut self) {
        for py in 0..4 { for px in 0..4 {
            if occupied(self.kind, self.rotation, px, py) {
                self.board[(self.y + py) as usize][(self.x + px) as usize] = self.kind + 1;
            }
        }}
        let mut target = HEIGHT;
        let mut cleared = 0;
        for row in (0..HEIGHT).rev() {
            if self.board[row].iter().all(|&cell| cell != 0) { cleared += 1; }
            else { target -= 1; self.board[target] = self.board[row]; }
        }
        for row in 0..target { self.board[row] = [0; WIDTH]; }
        self.score = self.score.saturating_add([0, 100, 300, 500, 800][cleared] * (self.level() + 1));
        self.lines += cleared as u32;
        self.kind = self.next;
        self.next = self.take_piece();
        self.rotation = 0; self.x = 3; self.y = 0;
        self.over = !self.fits(self.x, self.y, 0);
    }
    /// Compose a frame: 1..7 blocks, 9..15 ghost outline, 0 empty.
    pub fn frame(&self) -> [[u8; WIDTH]; HEIGHT] {
        let mut frame = self.board;
        if !self.over {
            let ghost = self.landing();
            for py in 0..4 { for px in 0..4 {
                if occupied(self.kind, self.rotation, px, py) {
                    frame[(ghost + py) as usize][(self.x + px) as usize] = self.kind + 9;
                    frame[(self.y + py) as usize][(self.x + px) as usize] = self.kind + 1;
                }
            }}
            // Ghost cells must never replace overlapping cells of the active piece.
            for py in 0..4 { for px in 0..4 {
                if occupied(self.kind, self.rotation, px, py) {
                    frame[(self.y + py) as usize][(self.x + px) as usize] = self.kind + 1;
                }
            }}
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn rotations_have_four_cells_and_cycle() {
        for kind in 0..7 { for rotation in 0..4 {
            assert_eq!((0..4).flat_map(|y| (0..4).map(move |x| (x,y))).filter(|&(x,y)| occupied(kind, rotation,x,y)).count(), 4);
            for y in 0..4 { for x in 0..4 { assert_eq!(occupied(kind,rotation,x,y), occupied(kind,rotation+4,x,y)); }}
        }}
    }
    #[test] fn walls_floor_and_hard_drop() {
        let mut game = Game::new(1);
        for _ in 0..20 { game.shift(-1); }
        assert!(game.fits(game.x,game.y,game.rotation));
        assert!(!game.fits(game.x-1,game.y,game.rotation));
        game.hard_drop();
        assert_eq!(game.board.iter().flatten().filter(|&&v|v!=0).count(),4);
        assert!(game.score > 0);
    }
    #[test] fn clears_four_rows_and_scores() {
        let mut game = Game::new(5);
        game.kind = 0; game.rotation = 1; game.x = 2; game.y = 0;
        for row in 16..20 { game.board[row] = [2;10]; game.board[row][4] = 0; }
        game.hard_drop();
        assert_eq!(game.lines,4);
        assert!(game.score>=800);
        assert!(game.board.iter().flatten().all(|&v|v==0));
    }
    #[test] fn game_over_and_restarting() {
        let mut game = Game::new(3);
        game.y = game.landing();
        for row in 0..4 { game.board[row] = [1;10]; game.board[row][0]=0; }
        game.down(false);
        assert!(game.over);
        let old = game.board;
        game.hard_drop(); game.shift(-1); game.rotate(true);
        assert_eq!(game.board,old);
        assert!(!Game::new(3).over);
    }
    #[test] fn seven_bag_and_deterministic_seed() {
        let mut game = Game::new(7);
        let mut other = Game::new(7);
        for _ in 0..30 { assert_eq!(game.take_piece(),other.take_piece()); }
        game.remaining=0;
        let mut seen=[false;7];
        for _ in 0..7 { seen[game.take_piece() as usize]=true; }
        assert!(seen.iter().all(|v|*v));
    }
    #[test] fn blocked_rotation_and_speed() {
        let mut game = Game::new(2);
        game.kind=2; game.x=3; game.y=4;
        game.board=[[1;10];20];
        for py in 0..4 { for px in 0..4 { if occupied(game.kind,0,px,py) {game.board[(game.y+py) as usize][(game.x+px) as usize]=0;} }}
        game.rotate(true);
        assert_eq!(game.rotation,0);
        game.lines=1000; assert_eq!(game.interval(),8);
    }
}
