#[path="../src/ui_font.rs"] mod ui_font;
use std::io::Write;
fn main() {std::io::stdout().write_all(&ui_font::FONT).unwrap();}
