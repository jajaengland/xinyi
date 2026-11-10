#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::{
    appearance::{self, Theme},
    gui, key,
    widgets::{Button, Rect, TextInput},
    window::{self, Draw},
};
const COLORS: [u32; 16] = [
    0x204a9c, 0x367c85, 0x3268b5, 0x4b80ba, 0x277d85, 0x28634f, 0x626184, 0x854a73, 0x40464f,
    0xc6d3dc, 0x969da5, 0xc4c4c4, 0xe8e8e8, 0x805534, 0x88473e, 0x193653,
];
struct Settings {
    draft: Theme,
    target: u8,
    hex: TextInput<6>,
    buttons: [Button; 8],
    volume: u32,
    editing: bool,
    status: &'static [u8],
    wallpaper:xinyi::wallpaper::Request,
    paper_buttons:[Button;3],
    dialog:Option<xinyi::file_dialog::Dialog>,
}
impl Settings {
    fn color(&self) -> u32 {
        if self.target == 0 {
            self.draft.desktop
        } else {
            self.draft.title
        }
    }
    fn choose(&mut self, color: u32) {
        if self.target == 0 {
            self.draft.desktop = color;
        } else {
            self.draft.title = color;
        }
        self.editing = false;
        self.hex.set(b"");
        self.status = b"Apply previews; Save persists on the mounted drive.";
    }
    fn apply(&mut self, save: bool) {
        let result=xinyi::wallpaper::apply(&self.wallpaper,save);
        if result<0{self.status=gui::report(2,match result{-8=>b"Unsupported or damaged image. Use an uncompressed 24/32-bit BMP.",-27=>b"Wallpaper BMP must be at most 768 KiB.",-116=>b"Drive changed. Choose the wallpaper again.",-2=>b"Wallpaper image was not found. Choose another file.",-12=>b"Not enough memory to prepare this wallpaper.",_=>b"Wallpaper could not be applied. Previous wallpaper was kept."});return;}
        let n = appearance::apply(&self.draft, save);
        if n == 0 && save {
            let mut data = [0; 8];
            data[..4].copy_from_slice(b"SND1");
            data[4..].copy_from_slice(&self.volume.to_le_bytes());
            if xinyi::files::save(b"/SOUND.CFG", &data) < 0 {
                self.status = xinyi::gui::report(2,b"Appearance saved; sound volume could not be saved.");
                xinyi::sounds::play(xinyi::sounds::Event::Error);
                return;
            }
        }
        self.status = match n {
            0 if save => xinyi::gui::report(0,b"Saved appearance, wallpaper and sound volume."),
            0 => xinyi::gui::report(0,b"Applied for this session. Save to keep after reboot."),
            -13 => xinyi::gui::report(2,b"GUI.CFG is read-only. Changes were not saved."),
            -19 => xinyi::gui::report(2,b"No mounted drive. Apply works without saving."),
            _ => xinyi::gui::report(2,b"Wallpaper applied; colours could not be saved."),
        };
        xinyi::sounds::play(if n == 0 {
            xinyi::sounds::Event::Complete
        } else {
            xinyi::sounds::Event::Error
        });
    }
    fn paint(&self, w: u32) {
        window::draw(
            w,
            &[
                Draw::rect(0, 0, 640, 416, 0xf3f6f8),
                Draw::rect(0,0,640,30,0x203e52),
                Draw::rect(8,34,464,204,0xffffff),
                Draw::rect(8,240,464,96,0xffffff),
                Draw::rect(0,382,640,1,0xc6d3dc),
                Draw::label(16, 7, 448, 0xffffff, b"Appearance"),
                Draw::label(16, 42, 252, 0, b"Desktop background"),
                Draw::label(16, 78, 252, 0, b"Active title bar"),
            ],
        );
        for (i, color) in [self.draft.desktop, self.draft.title].iter().enumerate() {
            let y = 36 + i as u32 * 36;
            window::draw(
                w,
                &[
                    Draw::rect(
                        284,
                        y,
                        180,
                        28,
                        if self.target == i as u8 {
                            0x367c85
                        } else {
                            0xc6d3dc
                        },
                    ),
                    Draw::rect(288, y + 4, 172, 20, *color),
                ],
            );
        }
        for (i, color) in COLORS.iter().enumerate() {
            let x = 16 + (i % 8) as u32 * 56;
            let y = 120 + (i / 8) as u32 * 40;
            window::draw(
                w,
                &[
                    Draw::rect(
                        x,
                        y,
                        48,
                        32,
                        if *color == self.color() { 0x367c85 } else { 0xe6edf2 },
                    ),
                    Draw::rect(x + 3, y + 3, 42, 26, *color),
                ],
            );
        }
        let encoded = self.draft.encode();
        let text = if self.editing {
            &self.hex.bytes[..self.hex.len]
        } else if self.target == 0 {
            &encoded[..6]
        } else {
            &encoded[7..13]
        };
        window::draw(
            w,
            &[
                Draw::label(16,212,80,0x20384d,b"RGB hex:"),
                Draw::label(232,212,232,0x536c80,b"6 hex digits; Enter to accept"),
            ],
        );
        let mut display = TextInput::<6>::new();
        display.set(text);
        gui::text_input(
            w,
            Rect::new(96, 206, 120, 28),
            if self.editing { &self.hex } else { &display },
            self.editing,
        );
        gui::outline(w,Rect::new(96,206,120,28),if self.editing {0x367c85}else{0xc6d3dc});
        let mut volume = *b"System sounds: 000%";
        volume[15] = b'0' + (self.volume / 100) as u8;
        volume[16] = b'0' + (self.volume / 10 % 10) as u8;
        volume[17] = b'0' + (self.volume % 10) as u8;
        window::draw(
            w,
            &[
                Draw::label(16,246,448,0x20384d,&volume),
                Draw::rect(16,268,448,4,0xe6edf2),Draw::rect(16,268,448*self.volume/100,4,0x367c85),
                Draw::label(16, 320, 448, 0, b"Sound volume applies now; Save keeps it."),
            ],
        );
        for (i, label) in [
            b"Apply" as &[u8],
            b"Save",
            b"Defaults",
            b"Close",
            b"-",
            b"+",
            if self.volume == 0 { b"Unmute" } else { b"Mute" },
            b"Test sound",
        ]
        .iter()
        .enumerate()
        {
            gui::button_flat(w,&self.buttons[i],label,i==1);
        }
        window::draw(
            w,
            &[Draw::text(
                16,
                392,
                0,
                &self.status[..self.status.len().min(56)],
            )],
        );
        window::draw(w,&[Draw::rect(480,34,152,338,0xffffff),Draw::label(488,42,136,0x20384d,b"Wallpaper")]);
        for (i,label) in [b"Choose image" as &[u8],match self.wallpaper.mode{0=>b"Mode: Fit",1=>b"Mode: Fill",_=>b"Mode: Center"},b"Use colour"].iter().enumerate(){gui::button_flat(w,&self.paper_buttons[i],label,false);}
        let path=&self.wallpaper.path[..self.wallpaper.length as usize];let leaf=path.rsplit(|b|*b==b'/').next().unwrap_or(b"");
        window::draw(w,&[Draw::label(488,196,136,0x20384d,if path.is_empty(){b"Default background"}else{leaf}),Draw::paragraph(488,224,136,96,0x536c80,b"24/32-bit BMP, up to 768 KiB. Apply previews; Save keeps the image choice.")]);
        if let Some(dialog)=self.dialog.as_ref(){dialog.paint(w);}
        window::present(w);
    }
}
fn main() -> u32 {
    let w = window::open_sized(b"Settings", 640, 416);
    if w < 0 {
        return 1;
    }
    let w = w as u32;
    let mut theme = Theme::DEFAULT;
    appearance::get(&mut theme);
    let mut paper=xinyi::wallpaper::Request::new();let wallpaper_loaded=xinyi::wallpaper::get(&mut paper);
    let mut s = Settings {
        wallpaper:paper,paper_buttons:[Button::new(Rect::new(488,72,136,28)),Button::new(Rect::new(488,112,136,28)),Button::new(Rect::new(488,152,136,28))],dialog:None,
        draft: theme,
        target: 0,
        volume: xinyi::sounds::volume().clamp(0, 100) as u32,
        hex: TextInput::new(),
        buttons: [
            Button::new(Rect::new(16, 344, 96, 28)),
            Button::new(Rect::new(128, 344, 96, 28)),
            Button::new(Rect::new(240, 344, 96, 28)),
            Button::new(Rect::new(352, 344, 96, 28)),
            Button::new(Rect::new(16, 282, 80, 28)),
            Button::new(Rect::new(104, 282, 80, 28)),
            Button::new(Rect::new(192, 282, 112, 28)),
            Button::new(Rect::new(312, 282, 136, 28)),
        ],
        editing: false,
        status: b"Choose a colour target, then a swatch or RGB value.",
    };
    if wallpaper_loaded==0&&paper.length>0{s.status=gui::report(1,b"Saved wallpaper is unavailable. Choose another image or use colour.");}
    let mut buttons = 0;
    s.paint(w);
    loop {
        let mut e = window::Event::default();
        if window::event(w, &mut e, true) < 0 {
            return 1;
        }
        if e.kind==window::CLOSE{return 0;}
        if let Some(dialog)=s.dialog.as_mut(){
            match dialog.event(e){xinyi::file_dialog::Result::Accept{path,len,volume}=>{s.wallpaper.path=path;s.wallpaper.length=len as u16;s.wallpaper.volume=volume;s.dialog=None;s.status=b"Image selected. Apply previews; Save keeps it.";},xinyi::file_dialog::Result::Cancel=>s.dialog=None,_=>()}
            s.paint(w);continue;
        }
        let mut repaint = false;
        match e.kind {
            window::CLOSE => return 0,
            window::RESET => {
                buttons = e.buttons;
                for b in s.buttons.iter_mut().chain(s.paper_buttons.iter_mut()) {
                    b.reset(e.buttons);
                }
                repaint=true;
            }
            window::KEY => {
                let code = e.code as u16;
                if code == 27 {
                    if s.editing {
                        s.editing = false;
                    } else {
                        return 0;
                    }
                } else if e.code & key::CTRL != 0 && matches!(code, 83 | 115) {
                    s.apply(true);
                } else if s.editing {
                    if code == 10 {
                        if let Some(color) = appearance::hex(&s.hex.bytes[..s.hex.len]) {
                            s.choose(color);
                        } else {
                            s.status = xinyi::gui::report(1,b"Enter exactly six hexadecimal digits.");
                        }
                    } else {
                        gui::edit_key(&mut s.hex, e.code);
                    }
                }
                repaint = true;
            }
            window::POINTER => {
                if e.buttons & 1 != 0 && buttons & 1 == 0 {
                    if (30..104).contains(&e.y) {
                        s.target = if e.y < 70 { 0 } else { 1 };
                        s.editing = false;
                    } else if (120..192).contains(&e.y) && (16..464).contains(&e.x) {
                        let col = (e.x - 16) / 56;
                        let row = (e.y - 120) / 40;
                        if col < 8 && (e.x - 16) % 56 < 48 && (e.y - 120) % 40 < 32 {
                            s.choose(COLORS[(row * 8 + col) as usize]);
                        }
                    } else if (96..216).contains(&e.x) && (206..234).contains(&e.y) {
                        s.editing = true;
                        s.hex.set(b"");
                    }
                }
                let old_pressed = s
                    .buttons
                    .iter()
                    .fold(0u32, |n, b| (n << 2) | u32::from(b.hover) | (u32::from(b.pressed && b.hover)<<1));
                let mut action = None;
                for (i, b) in s.buttons.iter_mut().enumerate() {
                    if b.pointer(e.x, e.y, e.buttons) {
                        action = Some(i);
                    }
                }
                if let Some(i) = action {
                    match i {
                        0 => s.apply(false),
                        1 => s.apply(true),
                        2 => {
                            s.draft = Theme::DEFAULT;s.wallpaper=xinyi::wallpaper::Request::new();s.wallpaper.volume=xinyi::directory::volume();
                            s.volume = 60;
                            xinyi::sounds::set_volume(s.volume);
                            s.editing = false;
                            s.status = b"Defaults selected. Apply or Save to use them.";
                        }
                        3 => return 0,
                        4 => {
                            s.volume = s.volume.saturating_sub(10);
                            xinyi::sounds::set_volume(s.volume);
                        }
                        5 => {
                            s.volume = (s.volume + 10).min(100);
                            xinyi::sounds::set_volume(s.volume);
                        }
                        6 => {
                            s.volume = if s.volume == 0 { 60 } else { 0 };
                            xinyi::sounds::set_volume(s.volume);
                        }
                        7 => {
                            xinyi::sounds::notification();
                        }
                        _ => (),
                    }
                }
                let mut paper_action=None;
                for(i,b)in s.paper_buttons.iter_mut().enumerate(){let before=(b.hover,b.pressed);if b.pointer(e.x,e.y,e.buttons){paper_action=Some(i);}if before!=(b.hover,b.pressed){repaint=true;}}
                if let Some(i)=paper_action{match i{0=>{let mut dialog=xinyi::file_dialog::Dialog::new(false);dialog.input.set(if s.wallpaper.length>0{&s.wallpaper.path[..s.wallpaper.length as usize]}else{b"/APPS/SETTINGS/WALL.BMP"});s.dialog=Some(dialog);},1=>s.wallpaper.mode=(s.wallpaper.mode+1)%3,_=>{s.wallpaper.length=0;s.wallpaper.path.fill(0);s.wallpaper.volume=xinyi::directory::volume();}}s.status=b"Apply previews the wallpaper; Save keeps it.";repaint=true;}
                let new_pressed = s
                    .buttons
                    .iter()
                    .fold(0u32, |n, b| (n << 2) | u32::from(b.hover) | (u32::from(b.pressed && b.hover)<<1));
                repaint = repaint || (e.buttons & 1 != 0 && buttons & 1 == 0)
                    || action.is_some()
                    || old_pressed != new_pressed;
                buttons = e.buttons;
            }
            _ => (),
        }
        if repaint {
            s.paint(w);
        }
    }
}
