#![no_std]
#![no_main]
mod selection;
mod browser;
mod chrome;
mod app_review;
use browser::{Favorites, History, Sort, View};
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use selection::Selection;
use xinyi::{
    directory::{self, Listing},
    file_clipboard::{self, Selection as FileClip},
    key,
    window::{self, Draw},
};
struct Files {
    review_buttons:[xinyi::widgets::Button;2],
    review:Option<xinyi::app_manifest::Manifest>,
    toolbar_hot:Option<usize>,
    handle: u32,
    view: View,
    favorites: Favorites,
    favorite_focus: usize,
    transfer_total: u16,
    transfer_done: u16,
    width: u32,
    height: u32,
    list: Listing,
    sort: Sort,
    cached: usize,
    history: History,
    name_end: u32,
    type_end: u32,
    column_drag: Option<u8>,
    property: Option<directory::Entry>,
    offset: u16,
    selection: Selection,
    path: [u8; 105],
    length: usize,
    volume: u32,
    status: &'static [u8],
    mode: u8,
    input: [u8; 12],
    used: usize,
    last: usize,
    clicked: u32,
    modifiers: u32,
    menu: Option<(i32, i32)>,
    hover: Option<usize>,
    can_paste: bool,
    pending: FileClip,
    quit: bool,
    press: Option<(i32, i32, usize, u32, bool)>,
    dragging: bool,
    open_on_release: bool,
    drop_batch: bool,
    destination: [u8; 105],
    destination_len: usize,
    drop_row: Option<usize>,
}
fn error(n: i32) -> &'static [u8] {
    xinyi::sounds::play(xinyi::sounds::Event::Error);
    match n {
        -2 => b"Item no longer exists. Refresh the folder.",
        -8 => b"Invalid app folder. Check its executable and matching .APP manifest.",
        -13 => b"Read-only item or folder.",
        -17 => b"That name already exists.",
        -20 => b"Not a folder.",
        -21 => b"Expected a regular file.",
        -22 => b"Invalid 8.3 path, or destination is inside the source.",
        -24 => b"Close another window before opening this item.",
        -27 => b"File/tree exceeds the supported size or depth.",
        -28 => b"The disk is full.",
        -39 => b"Folder is not empty.",
        -16 => b"Folder is in use. Close the app or leave its folder first.",
        -116 => b"Drive or file selection changed. Refresh and select again.",
        -12 => b"Not enough memory.",
        _ => b"Disk operation failed. Check remaining items.",
    }
}
impl Files {
    fn command_enabled(&self,n:usize)->bool {
        match n {0=>self.length>1,1=>self.selection.count==1,15=>self.history.target(false).is_some(),16=>self.history.target(true).is_some(),6=>self.offset>0,7=>self.offset as u32+(self.page() as u32)<self.list.total,_=>true}
    }
    fn command_button(&self,w:u32,r:chrome::Rect,n:usize,label:&[u8]) {
        let enabled=self.command_enabled(n);let hot=enabled&&self.toolbar_hot==Some(n);
        let bg=if hot {0xdceef0}else{0xe6edf2};let ink=if enabled {0x20384d}else {0x8293a1};
        let length=xinyi::text::width(label,xinyi::text::Font::Ui).min(r.w-12);
        window::draw(w,&[Draw::rect(r.x,r.y,r.w,r.h,bg),Draw::rect(r.x,r.y+r.h-1,r.w,1,if hot {0x367c85}else {0xc6d3dc}),Draw::label(r.x+(r.w-length)/2,r.y+(r.h-16)/2,length,ink,label)]);
    }
    fn page(&self) -> u16 {
        self.view.layout(self.width,self.height).3.max(1)
    }
    fn resize(&mut self, w: u32, e: window::Event) {
        if window::resize(w, e) == 0 {
            self.width = e.x as u32;
            self.name_end = self.name_end.min(self.width - 200);
            self.type_end = self.type_end.min(self.width - 104).max(self.name_end + 80);
            self.height = e.y as u32;
            self.menu = None;
            self.hover = None;
            self.last = usize::MAX;
            self.refresh();
        }
    }
    fn item_at(&self,x:i32,y:i32)->Option<usize> {self.view.hit(self.width,self.height,x,y,self.rows())}
    fn set_view(&mut self,view:View) {
        self.view=view;self.menu=None;self.press=None;self.column_drag=None;self.last=usize::MAX;
        self.offset=self.selection.focus.unwrap_or(self.offset)/self.page()*self.page();
        self.refresh();self.status=b"Ctrl+1 Details | Ctrl+2 Compact | Ctrl+3 Icons | Ctrl+D favourite | Ctrl+B Places";
    }
    fn load_favorites(&mut self)->bool {
        if self.volume!=directory::volume() {self.status=error(-116);return false;}
        let mut data=[0;1024];let n=xinyi::read_file(b"/FAVS.CFG",&mut data);
        if n == -2 {self.favorites=Favorites::new();return true;}
        if n<0 {self.status=error(n);return false;}
        if self.volume!=directory::volume() {self.status=error(-116);return false;}
        match Favorites::decode(&data[..n as usize]) {
            Some(f)=>{self.favorites=f;true},
            None=>{self.status=b"FAVS.CFG is invalid. It has been preserved.";false}
        }
    }
    fn toggle_favorite(&mut self) {
        if !self.load_favorites() {return;}
        let mut next=self.favorites;
        let added=match next.toggle(&self.path[..self.length]) {Ok(added)=>added,Err(())=>{self.status=b"Places holds eight folders. Remove one with Ctrl+D in that folder.";return;}};
        let mut data=[0;1024];let n=next.encode(&mut data);
        let result=xinyi::files::save_on(b"/FAVS.CFG",&data[..n],self.volume);
        if result<0 {self.status=error(result);return;}
        self.favorites=next;
        self.status=if added {b"Favourite saved. Open Places or press Ctrl+B."}else{b"Favourite removed."};
        self.refresh();
    }
    fn open_favorite(&mut self) {
        if self.volume!=directory::volume() {self.mode=0;self.refresh();self.status=error(-116);return;}
        if self.favorite_focus<self.favorites.count {
            let path=self.favorites.paths[self.favorite_focus];let n=self.favorites.lengths[self.favorite_focus] as usize;
            self.mode=0;self.navigate(&path[..n]);
        }
    }
    fn transfer_progress(&self,name:&[u8]) {
        self.paint(self.handle);
        let mut number=[b'0';5];let mut total=[b'0';5];
        let mut done=self.transfer_done;let mut all=self.transfer_total;
        for i in (0..5).rev() {number[i]+= (done%10) as u8;done/=10;total[i]+=(all%10) as u8;all/=10;}
        let a=number.iter().position(|&c|c!=b'0').unwrap_or(4);let b=total.iter().position(|&c|c!=b'0').unwrap_or(4);
        let bar=self.width-96;let fill=bar*self.transfer_done as u32/(self.transfer_total as u32).max(1);
        window::draw(self.handle,&[
            Draw::rect(24,112,self.width-48,160,0xc4c4c4),
            Draw::text(40,128,0,if self.pending.cut!=0 {b"Moving items"}else{b"Copying items"}),
            Draw::text(40,160,0,b"Completed:"),Draw::text(136,160,0,&number[a..]),Draw::text(184,160,0,b"of"),Draw::text(216,160,0,&total[b..]),
            Draw::text(40,192,0,&name[name.len().saturating_sub(68)..]),
            Draw::rect(40,224,bar,12,0x888888),Draw::rect(40,224,fill,12,0x238a60),
            Draw::text(40,248,0,b"Current item is in progress; I/O in the title shows disk activity."),
        ]);window::present(self.handle);
    }
    fn rows(&self) -> usize {
        (self.list.count as usize).min(self.page() as usize)
    }
    fn refresh(&mut self) {
        self.last = usize::MAX;
        let volume = directory::volume();
        if self.volume != 0 && self.volume != volume {
            self.history = History::new();
            self.selection.clear();
        }
        let mut n = if self.sort.column == 0 {
            if self.cached != 0 { xinyi::syscall(87, 0x40100000, 0, 0); }
            self.cached = 0;
            directory::list(b".", self.offset, &mut self.list)
        } else { self.sorted_page() };
        if n < 0 && self.sort.column != 0 {
            self.sort = Sort::default();
            self.cached = 0;
            xinyi::syscall(87, 0x40100000, 0, 0);
            self.selection.clear();
            self.status = b"Sort unavailable. Showing original disk order; click a heading to retry.";
            n = directory::list(b".", self.offset, &mut self.list);
        }
        if n < 0 {
            self.list = Listing::new();
            self.status = error(n);
        } else if self.offset as u32 >= self.list.total && self.offset > 0 {
            self.offset = 0;
            self.refresh();
            return;
        }
        let n = xinyi::working_directory(&mut self.path);
        self.length = n.max(0) as usize;
        if volume != directory::volume() {
            self.volume = 0;
            self.list = Listing::new();
            self.selection.clear();
            self.status = error(-116);
        } else {
            self.volume = volume;
            if self.history.count == 0 { self.history.record(&self.path[..self.length]); }
        }
    }
    fn sorted_page(&mut self) -> i32 {
        let mut page = Listing::new();
        let result = directory::list(b".", 0, &mut page);
        if result < 0 { return result; }
        let total = page.total as usize;
        let bytes = match total.checked_mul(core::mem::size_of::<directory::Entry>()) {
            Some(n) if n <= 1024 * 1024 && total <= u16::MAX as usize => n,
            _ => return -12,
        };
        self.cached = 0;
        let result = xinyi::syscall(87, 0x40100000 + bytes as u32, 0, 0);
        if result < 0 { return result; }
        let entries = unsafe { core::slice::from_raw_parts_mut(0x40100000 as *mut directory::Entry, total) };
        let mut at = 0;
        while at < total {
            if at != 0 {
                let n = directory::list(b".", at as u16, &mut page);
                if n < 0 { return n; }
            }
            if page.total as usize != total || page.count == 0 || page.count as usize > total - at { return -116; }
            let count = page.count as usize;
            entries[at..at + count].copy_from_slice(&page.entries[..count]);
            at += count;
        }
        let sort = self.sort;
        entries.sort_unstable_by(|a,b| sort.compare((&a.name[..a.length as usize],a.flags,a.size), (&b.name[..b.length as usize],b.flags,b.size)));
        self.cached = total;
        self.list = Listing::new();
        self.list.total = total as u32;
        let start = (self.offset as usize).min(total);
        let count = (total - start).min(32);
        self.list.entries[..count].copy_from_slice(&entries[start..start + count]);
        self.list.count = count as u32;
        0
    }
    fn read_selection_page(&self, offset: u16, page: &mut Listing) -> i32 {
        if self.sort.column == 0 { return directory::list(b".", offset, page); }
        let start = (offset as usize).min(self.cached);
        let count = (self.cached - start).min(32);
        *page = Listing::new();
        page.total = self.cached as u32;
        page.count = count as u32;
        let entries = unsafe { core::slice::from_raw_parts(0x40100000 as *const directory::Entry, self.cached) };
        page.entries[..count].copy_from_slice(&entries[start..start + count]);
        0
    }
    fn history_go(&mut self, forward: bool) {
        if self.volume != directory::volume() { self.refresh(); self.status = error(-116); return; }
        if let Some(index) = self.history.target(forward) {
            let path = self.history.paths[index];
            let n = directory::cd(&path[..self.history.lengths[index]]);
            if n < 0 { self.status = error(n); return; }
            self.history.cursor = index;
            self.offset = 0;
            self.selection.clear();
            self.refresh();
            self.status = b"History folder opened.";
        }
    }
    fn choose_sort(&mut self, column: u8) {
        self.sort.choose(column);
        self.offset = 0;
        self.selection.clear();
        self.status = b"Sorted across the whole folder. Click the heading to reverse.";
        self.refresh();
    }
    fn navigate(&mut self, name: &[u8]) {
        let n = directory::cd(name);
        if n < 0 {
            self.status = error(n);
        } else {
            self.offset = 0;
            self.selection.clear();
            self.status = b"Folder opened.";
            self.refresh();
            self.history.record(&self.path[..self.length]);
            if self.list.total == 0 {
                xinyi::sounds::play(xinyi::sounds::Event::Empty);
            }
        }
    }
    fn launch(&mut self, prefix: &[u8], name: &[u8]) {
        let mut line = [0; 128];
        let n = prefix.len() + name.len();
        line[..prefix.len()].copy_from_slice(prefix);
        line[prefix.len()..n].copy_from_slice(name);
        let n = xinyi::launch(&line[..n]);
        self.status = if n < 0 {
            error(n)
        } else {
            b"Opened. Use the taskbar to return to Files; F5 refreshes."
        };
    }
    fn open(&mut self) {
        if self.selection.count != 1 {
            self.status = b"Select one item to open.";
            return;
        }
        if self.volume != directory::volume() {
            self.status = error(-116);
            return;
        }
        let name = self.selection.names[0];
        let n = self.selection.lengths[0] as usize;
        if self.selection.flags[0] & 1 != 0 {
            self.navigate(&name[..n]);
        } else if (name[..n].ends_with(b".BIN") || name[..n].ends_with(b".ELF")) {
            self.launch(b"terminal run ", &name[..n]);
        } else {
            let result = xinyi::associations::open(&name[..n], self.volume);
            self.status = match result {
                0 => b"Opened with its associated application.",
                xinyi::associations::NO_HANDLER => {
                    b"No installed application handles this file type."
                }
                xinyi::associations::AMBIGUOUS => {
                    b"More than one app handles this type. Open the app explicitly."
                }
                _ => error(result),
            };
        }
    }
    fn select(&mut self, row: usize, ctrl: bool, shift: bool) {
        let e = self.list.entries[row];
        let index = self.offset + row as u16;
        if shift {
            let anchor = self.selection.anchor.unwrap_or(index);
            let a = anchor.min(index);
            let b = anchor.max(index);
            if b - a >= 32 {
                self.status = b"Select at most 32 items at a time.";
                return;
            }
            let mut page = Listing::new();
            let n = self.read_selection_page(a, &mut page);
            if n < 0 {
                self.status = error(n);
                return;
            }
            if page.count < (b - a + 1) as u32 {
                self.status = b"Folder changed. Refresh before selecting.";
                return;
            }
            self.selection.clear();
            for e in &page.entries[..(b - a + 1) as usize] {
                self.selection
                    .add(&e.name[..e.length as usize], e.flags as u8);
            }
            self.selection.anchor = Some(anchor);
            self.selection.focus = Some(index);
        } else if ctrl {
            if !self
                .selection
                .toggle(&e.name[..e.length as usize], e.flags as u8, index)
            {
                self.status = b"Select at most 32 items at a time.";
            }
        } else {
            self.selection
                .plain(&e.name[..e.length as usize], e.flags as u8, index);
        }
    }
    fn payload(&mut self, cut: bool) -> bool {
        if self.selection.count == 0 {
            self.status = b"Select files or folders first.";
            return false;
        }
        let r = &mut self.pending;
        *r = FileClip::new();
        r.count = self.selection.count as u16;
        r.cut = cut as u16;
        r.volume = self.volume;
        for i in 0..self.selection.count {
            let extra = usize::from(self.length > 1);
            let n = self.length + extra + self.selection.lengths[i] as usize;
            if n > 105 {
                self.status = error(-22);
                return false;
            }
            r.paths[i][..self.length].copy_from_slice(&self.path[..self.length]);
            if extra != 0 {
                r.paths[i][self.length] = b'/';
            }
            r.paths[i][self.length + extra..n]
                .copy_from_slice(&self.selection.names[i][..self.selection.lengths[i] as usize]);
            r.lengths[i] = n as u16;
            r.flags[i] = self.selection.flags[i];
        }
        true
    }
    fn stage(&mut self, cut: bool) {
        if !self.payload(cut) {
            return;
        }
        let r = &mut self.pending;
        let n = file_clipboard::set(r);
        self.status = if n < 0 {
            error(n)
        } else if cut {
            b"Move selected. Open the destination, in any Files window, then Paste."
        } else {
            b"Copy selected. Open the destination, in any Files window, then Paste."
        };
    }
    fn paste(&mut self) {self.paste_checked(0);}
    fn paste_checked(&mut self,expected:u32) {
        self.drop_batch = false;
        self.destination_len = 0;
        let n = file_clipboard::get(&mut self.pending);
        if n < 0 {
            self.status = error(n);
            return;
        }
        if expected!=0&&self.pending.version!=expected {self.status=error(-116);return;}
        if self.pending.count == 0 {
            self.status = b"Copy or Move files first.";
            return;
        }
        self.selection.clear();
        self.transfer_total=self.pending.count;self.transfer_done=0;
        self.paste_next(None);
    }
    fn paste_next(&mut self, renamed: Option<&[u8]>) {
        let mut override_name = renamed;
        while self.pending.count > 0 {
            let path = self.pending.paths[0];
            let n = self.pending.lengths[0] as usize;
            let start = path[..n]
                .iter()
                .rposition(|&b| b == b'/')
                .map_or(0, |i| i + 1);
            let name = override_name.take().unwrap_or(&path[start..n]);
            let mut destination = [0; 105];
            let extra = usize::from(self.destination_len > 1);
            let len = self.destination_len + extra + name.len();
            let dest = if self.destination_len == 0 {
                name
            } else {
                if len > 105 {
                    self.status = error(-22);
                    self.finish_drop();
                    return;
                }
                destination[..self.destination_len]
                    .copy_from_slice(&self.destination[..self.destination_len]);
                if extra != 0 {
                    destination[self.destination_len] = b'/';
                }
                destination[self.destination_len + extra..len].copy_from_slice(name);
                &destination[..len]
            };
            self.transfer_progress(&path[..n]);
            let result = directory::transfer_on(
                &path[..n],
                dest,
                if self.pending.cut != 0 { 2 } else { 1 },
                self.pending.volume,
            );
            if result == -17 {
                self.mode = 5;
                self.used = 0;
                self.status = b"Name exists. Paste as another name, or Escape to stop this batch.";
                xinyi::sounds::play(xinyi::sounds::Event::Warning);
                self.refresh();
                return;
            }
            if result < 0 {
                self.finish_drop();
                self.status = error(result);
                self.refresh();
                return;
            }
            self.selection.add(name, self.pending.flags[0]);
            for i in 1..self.pending.count as usize {
                self.pending.paths[i - 1] = self.pending.paths[i];
                self.pending.lengths[i - 1] = self.pending.lengths[i];
                self.pending.flags[i - 1] = self.pending.flags[i];
            }
            self.pending.count -= 1;
            self.transfer_done+=1;
            if self.pending.cut != 0 && !self.drop_batch {
                let n = file_clipboard::update(&self.pending);
                if n < 0 {
                    self.status =
                        b"Moved an item; shared selection changed. Remaining moves stopped.";
                    self.refresh();
                    return;
                }
                self.pending.version = n as u32;
            }
        }
        if self.destination_len > 0
            && !self.destination[..self.destination_len]
                .eq_ignore_ascii_case(&self.path[..self.length])
        {
            self.selection.clear();
        }
        self.finish_drop();
        self.status = b"Paste complete.";
        xinyi::sounds::play(xinyi::sounds::Event::Complete);
        self.refresh();
    }
    fn finish_drop(&mut self) {
        if self.drop_batch {
            xinyi::drag_drop::finish();
            self.drop_batch = false;
        }
        self.destination_len = 0;
        self.drop_row = None;
    }
    fn receive_recycle(&mut self,e:window::Event) {
        let mut payload=FileClip::new();payload.version=e.code;
        let result=xinyi::drag_drop::take(&mut payload);xinyi::drag_drop::finish();
        if result<0 {self.status=error(result);return;}
        self.selection.clear();
        if payload.volume!=self.volume||self.volume!=directory::volume() {self.status=error(-116);return;}
        for i in 0..payload.count as usize {
            let path=&payload.paths[i][..payload.lengths[i] as usize];
            let Some(at)=path.iter().rposition(|b|*b==b'/') else{self.status=error(-22);self.selection.clear();return;};
            let parent=if at==0 {&path[..1]}else{&path[..at]};let name=&path[at+1..];
            if !parent.eq_ignore_ascii_case(&self.path[..self.length])||name.is_empty()||name.len()>12 {self.status=error(-22);self.selection.clear();return;}
            self.selection.add(name,payload.flags[i]);
        }
        if self.selection.count>0 {self.mode=3;self.status=b"Confirm recycling this desktop selection. Y continues / N cancels.";xinyi::sounds::play(xinyi::sounds::Event::Warning);}
    }
    fn receive_drop(&mut self, e: window::Event) {
        if self.mode != 0 {
            xinyi::drag_drop::finish();
            self.status = b"Drop cancelled. Finish the current dialog first.";
            return;
        }
        self.pending = FileClip::new();
        self.pending.version = e.code;
        let n = xinyi::drag_drop::take(&mut self.pending);
        if n < 0 {
            self.status = error(n);
            xinyi::drag_drop::finish();
            return;
        }
        self.drop_batch = true;
        self.dragging = false;
        self.press = None;
        self.drop_row = None;
        if self.mode != 0 || self.volume != directory::volume() {
            self.status = b"Drop cancelled: folder or dialog changed.";
            self.finish_drop();
            return;
        }
        self.destination = self.path;
        self.destination_len = self.length;
        if let Some(row) = self.item_at(e.x,e.y) {
            if self.list.entries[row].flags & 1 != 0 {
                let item = self.list.entries[row];
                let n = item.length as usize;
                let extra = usize::from(self.destination_len > 1);
                if self.destination_len + extra + n > 105 {
                    self.status = error(-22);
                    self.finish_drop();
                    return;
                }
                if extra != 0 {
                    self.destination[self.destination_len] = b'/';
                    self.destination_len += 1;
                }
                self.destination[self.destination_len..self.destination_len + n]
                    .copy_from_slice(&item.name[..n]);
                self.destination_len += n;
            }
        }
        // Dropping into the source folder is a no-op, never a rename prompt.
        if (0..self.pending.count as usize).all(|i| {
            let p = &self.pending.paths[i][..self.pending.lengths[i] as usize];
            p.iter().rposition(|&b| b == b'/').is_some_and(|n| {
                let parent = if n == 0 { &p[..1] } else { &p[..n] };
                parent.eq_ignore_ascii_case(&self.destination[..self.destination_len])
            })
        }) {
            self.status = b"Already in this folder.";
            self.finish_drop();
            return;
        }
        self.selection.clear();
        self.transfer_total=self.pending.count;self.transfer_done=0;
        self.paste_next(None);
    }
    fn delete(&mut self) {
        let s = self.selection;
        for i in 0..s.count {
            let mut path=[0;105];let extra=usize::from(self.length>1);let len=self.length+extra+s.lengths[i] as usize;
            if len>105 {self.status=error(-22);return;}
            path[..self.length].copy_from_slice(&self.path[..self.length]);if extra!=0 {path[self.length]=b'/';}path[self.length+extra..len].copy_from_slice(&s.names[i][..s.lengths[i] as usize]);
            let n=xinyi::trash::recycle(&path[..len],s.flags[i]&1!=0,self.volume);
            if n < 0 {
                self.status = error(n);
                self.selection.clear();
                self.refresh();
                return;
            }
        }
        self.status = b"Moved to Recycle Bin. Open Recycle Bin from Apps to restore.";
        xinyi::sounds::play(xinyi::sounds::Event::Complete);
        self.selection.clear();
        self.refresh();
    }
    fn manage_app(&mut self, operation: u16) {
        if self.selection.count != 1 || self.selection.flags[0] & 1 == 0 {
            self.status = b"Select one application folder.";
            return;
        }
        let name = self.selection.names[0];
        let n = self.selection.lengths[0] as usize;
        if operation!=1 && self.review.is_some() {
            if app_review::load(&name[..n],self.volume).ok()!=self.review {self.review=None;self.status=b"App metadata changed. Review the package again before continuing.";return;}
        }
        let result = directory::app_action(&name[..n], operation, self.volume);
        self.status = if result < 0 {
            error(result)
        } else {
            xinyi::sounds::play(xinyi::sounds::Event::Complete);
            if operation == 1 {
                b"App removed. Your documents are unchanged."
            } else if operation == 2 {
                b"App updated. Backup kept: use app rollback or app finalize in Terminal."
            } else {
                b"App installed. Open Apps to launch it."
            }
        };
        if result >= 0 {
            self.selection.clear();
        }
        self.refresh();
    }
    fn action(&mut self, n: usize) -> bool {
        self.menu = None;
        self.hover = None;
        if matches!(n, 4 | 10 | 11) && self.selection.count == 0 {
            self.status = b"Select files or folders first.";
            return false;
        }
        match n {
            0 => self.navigate(b".."),
            1 => self.open(),
            2 | 3 => {
                self.mode = if n == 2 { 1 } else { 2 };
                self.used = 0;
            }
            4 => {
                self.mode = 3;
                xinyi::sounds::play(xinyi::sounds::Event::Warning);
            }
            5 => {
                self.selection.clear();
                self.refresh();
            }
            6 => {
                self.offset = self.offset.saturating_sub(self.page());
                self.refresh();
            }
            7 => {
                if self.offset as u32 + (self.page() as u32) < self.list.total {
                    self.offset += self.page();
                    self.refresh();
                }
            }
            8 => self.set_view(match self.view {View::Details=>View::Compact,View::Compact=>View::Icons,View::Icons=>View::Details}),
            9 => {
                if self.selection.count != 1 {
                    self.status = b"Select one item to rename.";
                } else {
                    self.mode = 4;
                    self.used = 0;
                }
            }
            10 => self.stage(false),
            11 => self.stage(true),
            12 => self.paste(),
            13 | 14 | 19 => {
                if self.selection.count != 1 || self.selection.flags[0] & 1 == 0 {
                    self.status = b"Select one application folder.";
                } else if n == 14 && !self.path[..self.length].eq_ignore_ascii_case(b"/APPS") {
                    self.status = b"Open /APPS and select an installed app to remove.";
                } else {
                    let id=&self.selection.names[0][..self.selection.lengths[0] as usize];
                    self.review=match app_review::load(id,self.volume) {
                        Ok(metadata)=>Some(metadata),
                        Err(_) if n==14=>None,
                        Err(code)=>{self.status=error(code);return false;},
                    };
                    for button in &mut self.review_buttons {button.reset(1);}
                    self.mode = if n == 13 { 6 } else if n == 19 {10} else { 7 };
                    xinyi::sounds::play(xinyi::sounds::Event::Warning);
                }
            }
            20 => {
                self.can_paste=file_clipboard::get(&mut self.pending)==0&&self.pending.count>0;
                self.menu=Some((self.width as i32-176,32));self.last=usize::MAX;
            },
            18 => {if self.load_favorites() {self.favorite_focus=0;self.mode=9;}},
            15 => self.history_go(false),
            16 => self.history_go(true),
            17 => {
                if self.selection.count != 1 { self.status = b"Select one item for properties."; }
                else if self.volume != directory::volume() { self.status = error(-116); }
                else {
                    let name = self.selection.names[0]; let len = self.selection.lengths[0] as usize;
                    let mut page = Listing::new(); let mut offset = 0;
                    self.property = None;
                    loop {
                        let n = directory::list(b".", offset, &mut page);
                        if n < 0 { self.status = error(n); break; }
                        self.property = page.entries[..page.count as usize].iter().find(|e| &e.name[..e.length as usize] == &name[..len]).copied();
                        if self.property.is_some() { self.mode = 8; break; }
                        let next = offset as u32 + page.count;
                        if page.count == 0 || next >= page.total || next > u16::MAX as u32 { self.status = error(-2); break; }
                        offset = next as u16;
                    }
                }
            }
            _ => (),
        }
        false
    }
    fn key(&mut self, event: u32) -> bool {
        self.modifiers = event & (key::CTRL | key::SHIFT);
        let code = event as u16;
        let ctrl = event & key::CTRL != 0;
        if self.menu.is_some() {
            if code == 27 {
                self.menu = None;
                return false;
            }
            if code == 10 {
                if let Some(i) = self.hover.filter(|&i|self.enabled(i)) {
                    return self.action([1, 9, 10, 11, 12, 4, 5, 13, 14, 17, 19][i]);
                }
            }
            if matches!(code, key::UP | key::DOWN) {
                self.hover = Some(match (self.hover, code) {
                    (Some(i), key::UP) => i.saturating_sub(1),
                    (Some(i), _) => (i + 1).min(10),
                    _ => 0,
                });
            }
            return false;
        }
        if self.mode == 9 {
            match code {
                27 => self.mode=0,
                key::UP => self.favorite_focus=self.favorite_focus.saturating_sub(1),
                key::DOWN => self.favorite_focus=(self.favorite_focus+1).min(self.favorites.count.saturating_sub(1)),
                10 => self.open_favorite(),
                49..=56 if ((code-49) as usize)<self.favorites.count => {self.favorite_focus=(code-49) as usize;self.open_favorite();},
                _=>(),
            }
            return false;
        }
        if self.mode == 8 {
            if matches!(code, 27 | 10) { self.mode = 0; }
            return false;
        }
        if self.mode != 0 {
            if code == 27 {
                self.finish_drop();
                self.mode = 0;
                self.pending.count = 0;
                self.status = b"Cancelled. Earlier batch items may have completed.";
                return false;
            }
            if matches!(self.mode, 3 | 6 | 7 | 10) {
                if matches!(code, 89 | 121) {
                    let mode = self.mode;
                    self.mode = 0;
                    if mode == 3 {
                        self.delete();
                    } else {
                        self.manage_app(if mode==7 {1}else if mode==10 {2}else{0});
                    }
                } else if matches!(code, 78 | 110) {
                    self.mode = 0;
                    self.status = b"Cancelled.";
                }
                return false;
            }
            match code {
                8 => self.used = self.used.saturating_sub(1),
                10 if self.used > 0 => {
                    let name = self.input;
                    let used = self.used;
                    let mode = self.mode;
                    self.mode = 0;
                    match mode {
                        1 => self.launch(b"notepad ", &name[..used]),
                        2 => {
                            let n = directory::action_on(&name[..used], 0, self.volume);
                            self.status = if n < 0 { error(n) } else { b"Folder created." };
                            self.refresh();
                        }
                        4 => {
                            let s = self.selection;
                            let n = directory::transfer_on(
                                &s.names[0][..s.lengths[0] as usize],
                                &name[..used],
                                0,
                                self.volume,
                            );
                            self.status = if n < 0 { error(n) } else { b"Renamed." };
                            if n == 0 {
                                self.selection.clear();
                                self.selection.add(&name[..used], s.flags[0]);
                            }
                            self.refresh();
                        }
                        5 => self.paste_next(Some(&name[..used])),
                        _ => (),
                    }
                }
                32..=126 if !ctrl => {
                    if self.used < 12 {
                        self.input[self.used] = code as u8;
                        self.used += 1;
                    }
                }
                _ => (),
            }
            return false;
        }
        if event & key::ALT != 0 {
            match code { key::LEFT => self.history_go(false), key::RIGHT => self.history_go(true), 10 => { self.action(17); }, _ => () }
            return false;
        }
        if ctrl {
            match code {
                49 => {self.set_view(View::Details);return false;},
                50 => {self.set_view(View::Compact);return false;},
                51 => {self.set_view(View::Icons);return false;},
                68 | 100 => {self.toggle_favorite();return false;},
                66 | 98 => {self.action(18);return false;},
                48 => { self.sort = Sort::default(); self.selection.clear(); self.offset = 0; self.refresh(); return false; },
                81 | 113 => return true,
                78 | 110 => return self.action(2),
                67 | 99 => return self.action(10),
                88 | 120 => return self.action(11),
                86 | 118 => return self.action(12),
                65 | 97 => {
                    if self.list.total > 32 {
                        self.status = b"Select at most 32 items at a time.";
                    } else {
                        let mut page = Listing::new();
                        if self.read_selection_page(0, &mut page) == 0 {
                            self.selection.clear();
                            for e in &page.entries[..page.count as usize] {
                                self.selection
                                    .add(&e.name[..e.length as usize], e.flags as u8);
                            }
                        }
                    }
                    return false;
                }
                _ => (),
            }
        }
        match code {
            0x121 => {
                self.action(9);
            }
            key::UP | key::DOWN | key::LEFT | key::RIGHT | key::HOME | key::END => {
                if self.rows() > 0 {
                    let old = self.selection.focus.unwrap_or(self.offset);
                    let columns=self.view.layout(self.width,self.height).0 as u16;
                    let index = match code {
                        key::UP => old.saturating_sub(columns),
                        key::LEFT => old.saturating_sub(1),
                        key::DOWN | key::RIGHT => {
                            if self.selection.focus.is_none() {
                                self.offset
                            } else {
                                old.saturating_add(if code==key::DOWN {columns}else{1})
                                    .min(self.list.total.saturating_sub(1) as u16)
                            }
                        }
                        key::HOME => 0,
                        _ => self.list.total.saturating_sub(1) as u16,
                    };
                    if index < self.offset || index >= self.offset + self.page() {
                        self.offset = index / self.page() * self.page();
                        self.refresh();
                    }
                    let row = (index - self.offset) as usize;
                    if row < self.rows() {
                        self.select(row, false, event & key::SHIFT != 0);
                    }
                }
            }
            10 => self.open(),
            8 => self.navigate(b".."),
            key::DELETE => {
                self.action(4);
            }
            key::PAGE_UP => {
                self.action(6);
            }
            key::PAGE_DOWN => {
                self.action(7);
            }
            key::F5 => {
                self.selection.clear();
                self.refresh();
            }
            _ => (),
        }
        false
    }
    fn enabled(&self, i: usize) -> bool {
        match i {
            10 => self.selection.count == 1 && self.selection.flags[0] & 1 != 0,
            9 => self.selection.count == 1,
            0 | 1 => self.selection.count == 1,
            2 | 3 | 5 => self.selection.count > 0,
            4 => self.can_paste,
            7 | 8 => {
                self.selection.count == 1
                    && self.selection.flags[0] & 1 != 0
                    && (i == 7 || self.path[..self.length].eq_ignore_ascii_case(b"/APPS"))
            }
            _ => true,
        }
    }
    fn menu_hit(&self, x: i32, y: i32) -> Option<usize> {
        let (a, b) = self.menu?;
        if (a + 4..a + 164).contains(&x) && (b + 4..b + 246).contains(&y) {
            Some(((y - b - 4) / 22) as usize)
        } else {
            None
        }
    }
    fn pointer(&mut self, event: window::Event, previous: u32) -> bool {
        if matches!(self.mode,6|7|10) {
            let mut clicked=None;let mut changed=false;
            for (i,button) in self.review_buttons.iter_mut().enumerate() {
                button.rect=xinyi::widgets::Rect::new(self.width as i32-if i==0 {232}else{128},312,88,24);
                let before=(button.hover,button.pressed);
                if button.pointer(event.x,event.y,event.buttons) {clicked=Some(i);}
                changed|=before!=(button.hover,button.pressed);
            }
            if let Some(i)=clicked {self.key(if i==0 {b'y' as u32}else{b'n' as u32});return true;}
            return changed;
        }
        if self.column_drag.is_some() && event.buttons & 1 == 0 { self.column_drag = None; return true; }
        if event.buttons & 1 == 0 && previous & 1 != 0 {
            if let Some((_, _, row, mods, selected)) = self.press.take() {
                if !self.dragging && mods & key::SHIFT == 0 && row < self.rows() {
                    if mods & key::CTRL == 0 {
                        self.select(row, false, false);
                    } else if selected {
                        self.select(row, true, false);
                    }
                }
            }
            if !self.dragging && self.open_on_release {
                self.open();
            }
            self.open_on_release = false;
            self.dragging = false;
            return true;
        }
        if event.buttons & 1 != 0 && !self.dragging {
            if let Some((x, y, _, _, _)) = self.press {
                if (event.x - x).abs().max((event.y - y).abs()) >= 5 {
                    self.press = None;
                    self.open_on_release = false;
                    self.last = usize::MAX;
                    if self.payload(event.code & key::SHIFT != 0) {
                        let n = xinyi::drag_drop::begin(&self.pending);
                        if n == 0 {
                            self.dragging = true;
                            self.status=b"Drag to Files. Copy by default; hold Shift to move. Escape cancels.";
                        } else {
                            self.status = error(n);
                        }
                    }
                    return true;
                }
            }
        }
        let left = event.buttons & 1 != 0 && previous & 1 == 0;
        let right = event.buttons & 2 != 0 && previous & 2 == 0;
        if self.mode == 9 {
            if left && (144..320).contains(&event.y) && (24..self.width as i32-24).contains(&event.x) {
                let index=((event.y-144)/22) as usize;
                if index<self.favorites.count {self.favorite_focus=index;self.open_favorite();return true;}
            }
            return false;
        }
        if self.mode == 8 {
            if left { self.mode = 0; return true; }
            return false;
        }
        if self.mode != 0 { return false; }
        if let Some(column) = self.column_drag {
            if event.buttons & 1 == 0 { self.column_drag = None; }
            else if column == 1 { self.name_end = (event.x.max(120) as u32).min(self.type_end - 80); }
            else { self.type_end = (event.x.max((self.name_end + 80) as i32) as u32).min(self.width - 104); }
            return true;
        }
        if self.menu.is_some() {
            if left {
                let hit = self.menu_hit(event.x, event.y);
                self.menu = None;
                if let Some(i) = hit {
                    if self.enabled(i) {
                        self.action([1, 9, 10, 11, 12, 4, 5, 13, 14, 17, 19][i]);
                    }
                }
                return true;
            }
            if !right {
                let hover = self.menu_hit(event.x, event.y);
                let changed = hover != self.hover;
                self.hover = hover;
                return changed;
            }
        }
        let row = self.item_at(event.x,event.y);
        if right {
            if let Some(row) = row {
                let e = self.list.entries[row];
                if !self.selection.contains(&e.name[..e.length as usize]) {
                    self.select(row, false, false);
                }
            } else {
                self.selection.clear();
            }
            self.can_paste = file_clipboard::get(&mut self.pending) == 0 && self.pending.count > 0;
            self.menu = Some((
                event.x.clamp(0, self.width as i32 - 168),
                event.y.clamp(0, self.height as i32 - 250),
            ));
            self.hover = None;
            self.last = usize::MAX;
            return true;
        }
        let hot=chrome::hit(self.width,self.height,event.x,event.y);
        let changed=hot!=self.toolbar_hot;self.toolbar_hot=hot;
        if !left {return changed;}
        if let Some(action)=hot {if self.command_enabled(action) {self.action(action);}return true;}
        if self.view == View::Details && (64..80).contains(&event.y) {
            if (event.x - self.name_end as i32).abs() <= 4 { self.column_drag = Some(1); }
            else if (event.x - self.type_end as i32).abs() <= 4 { self.column_drag = Some(2); }
            else { self.choose_sort(if event.x < self.name_end as i32 { 1 } else if event.x < self.type_end as i32 { 2 } else { 3 }); }
            return true;
        }
        if (80..self.height as i32 - 32).contains(&event.y) {
            if let Some(row) = row {
                let modifiers = event.code;
                let ctrl = modifiers & key::CTRL != 0;
                let shift = modifiers & key::SHIFT != 0;
                let e = self.list.entries[row];
                let selected = self.selection.contains(&e.name[..e.length as usize]);
                if shift || !selected {
                    self.select(row, ctrl, shift);
                }
                self.press = Some((event.x, event.y, row, modifiers, selected));
                let now = xinyi::ticks();
                if !ctrl && !shift && self.last == row && now.wrapping_sub(self.clicked) < 30 {
                    self.open_on_release = true;
                    self.last = usize::MAX;
                } else {
                    self.last = if ctrl || shift { usize::MAX } else { row };
                    self.clicked = now;
                }
            } else {
                self.selection.clear();
                self.last = usize::MAX;
                self.clicked = 0;
            }
            return true;
        }
        false
    }
    fn paint(&self, w: u32) {
        window::draw(w, &[Draw::rect(0, 0, self.width, self.height, 0xf4f4f4)]);
        window::draw(w,&[Draw::rect(0,0,self.width,64,0xf3f6f8),Draw::rect(0,63,self.width,1,0xc6d3dc)]);
        for (i,(n,label)) in chrome::PRIMARY.iter().enumerate() {self.command_button(w,chrome::primary(self.width,i),*n,label);}
        for (i,(n,label)) in chrome::NAV.iter().enumerate() {self.command_button(w,chrome::nav(i),*n,label);}
        let budget=((self.width-232)/8).min(96) as usize;
        window::draw(w,&[Draw::rect(216,36,self.width-224,24,0xffffff),Draw::text(224,40,0x20384d,&self.path[self.length.saturating_sub(budget)..self.length]),Draw::rect(216,59,self.width-224,1,0xc6d3dc)]);
        window::draw(
            w,
            &[
                Draw::text(8, 64, 0, if self.sort.column == 1 { if self.sort.descending { b"Name -" } else { b"Name +" } } else { b"Name" }),
                Draw::text(self.name_end + 8, 64, 0, if self.sort.column == 2 { if self.sort.descending { b"Type -" } else { b"Type +" } } else { b"Type" }),
                Draw::text(self.type_end + 8, 64, 0, if self.sort.column == 3 { if self.sort.descending { b"Bytes -" } else { b"Bytes +" } } else { b"Bytes" }),
                Draw::rect(self.name_end, 64, 1, 16, 0x808080),
                Draw::rect(self.type_end, 64, 1, 16, 0x808080),
                Draw::rect(8, 80, self.width - 16, self.height - 112, 0xffffff),
            ],
        );
        if self.view != View::Details {
            window::draw(w,&[Draw::rect(0,64,self.width,16,0xf4f4f4),Draw::text(8,64,0,if self.view==View::Icons {b"Icons | Ctrl+1 Details | Ctrl+2 Compact | Ctrl+3 Icons"}else{b"Compact | Ctrl+1 Details | Ctrl+2 Compact | Ctrl+3 Icons"})]);
        }
        for i in 0..self.rows() {
            let e = self.list.entries[i];
            let name = &e.name[..e.length as usize];
            let chosen = self.selection.contains(name);
            let (bg, fg) = if chosen {
                (0x2b5268, 0xffffff)
            } else {
                (0xffffff, 0)
            };
            if self.view != View::Details {
                let (x,y,cw,ch)=self.view.cell(self.width,self.height,i);
                window::draw(w,&[Draw::rect(x,y,cw,ch,bg)]);
                let (ix,iy,iw,ih)=if self.view==View::Icons {(x+cw/2-14,y+8,28,28)}else{(x+4,y+5,12,14)};
                let folder=e.flags&1!=0;
                window::draw(w,&[
                    Draw::rect(ix,iy,iw,ih,if folder {0xc58e28}else{0x9aafc0}),
                    Draw::rect(ix+2,iy+2,iw-4,ih-4,if folder {0xf4cb69}else{0xf5f8ff}),
                    Draw::text(if self.view==View::Icons {x+(cw-name.len() as u32*8)/2}else{x+22},if self.view==View::Icons {y+48}else{y+4},fg,name),
                ]);
                continue;
            }
            let y = 80 + i as u32 * 16;
            let mut digits = [b' '; 10];
            let mut value = e.size;
            for n in (0..10).rev() {
                digits[n] = b'0' + (value % 10) as u8;
                value /= 10;
            }
            let start = digits.iter().position(|&b| b != b'0').unwrap_or(9);
            digits[..start].fill(b' ');
            window::draw(
                w,
                &[
                    Draw::rect(8, y, self.width - 16, 16, bg),
                    Draw::text(8, y, fg, name),
                    Draw::text(
                        self.name_end + 8,
                        y,
                        fg,
                        if e.flags & 1 != 0 { b"Folder" } else { b"File" },
                    ),
                    Draw::text(self.type_end + 8, y, fg, &digits),
                ],
            );
        }
        if let Some(row) = self.drop_row {
            let (x,y,cw,ch) = self.view.cell(self.width,self.height,row);
            window::draw(
                w,
                &[
                    Draw::rect(x, y, cw, 2, 0x238a60),
                    Draw::rect(x, y + ch - 2, cw, 2, 0x238a60),
                ],
            );
        }
        let hint = match self.mode {
            1 => b"New file (8.3): " as &[u8],
            2 => b"New folder (8.3): ",
            3 => b"Move selection to Recycle Bin? Y confirms / N cancels",
            4 => b"Rename to (8.3): ",
            5 => b"Paste as (8.3): ",
            6 => b"Install selected app folder? Y confirms / N cancels",
            7 => b"Remove app and its bundled files? Y confirms / N cancels",
            10 => b"Update installed app from this folder? Y confirms / N cancels",
            _ => b"Ctrl/Shift select | Ctrl+C/X/V | F2 rename | Right-click actions",
        };
        window::draw(
            w,
            &[
                Draw::label(8, self.height - 32, if self.mode==0 {self.width-176}else{self.width-16}, 0x20384d, if self.mode == 0 {b"PgUp / PgDn to browse"} else {hint}),
                Draw::text(
                    8,
                    self.height - 16,
                    0,
                    &self.status[..self
                        .status
                        .len()
                        .min(((self.width as usize - 16) / 8).min(96))],
                ),
            ],
        );
        if self.mode==0 {for next in [false,true] {self.command_button(w,chrome::page(self.width,self.height,next),if next {7}else{6},if next {b"Next"}else{b"Previous"});}}
        if matches!(self.mode, 1 | 2 | 4 | 5) {
            window::draw(
                w,
                &[Draw::text(
                    160,
                    self.height - 32,
                    0x2b5268,
                    &self.input[..self.used],
                )],
            );
        }
        if matches!(self.mode,6|7|10) {
            use xinyi::widgets::Rect;
            xinyi::gui::dialog_flat(w,if self.mode==6 {b"Review app installation"}else if self.mode==10 {b"Review app update"}else {b"Remove installed app?"},Rect::new(24,96,self.width as i32-48,248));
            if let Some(metadata)=self.review {
                let mut version=[b'0';10];let mut value=metadata.version;for digit in version.iter_mut().rev() {*digit+= (value%10) as u8;value/=10;}
                let start=version.iter().position(|&b|b!=b'0').unwrap_or(9);
                let (profile,permissions)=app_review::profile(metadata.access);
                window::draw(w,&[Draw::label(40,132,self.width-80,0x20384d,metadata.title()),Draw::text(40,156,0,b"Version:"),Draw::text(120,156,0,if metadata.version==0 {b"Legacy"}else{&version[start..]}),Draw::label(240,156,self.width-280,0x20384d,profile),Draw::label(40,180,self.width-80,0x536c80,metadata.description()),Draw::paragraph(40,232,self.width-80,40,0x20384d,permissions)]);
            } else {window::draw(w,&[Draw::label(40,140,self.width-80,0,b"Metadata unavailable. Access profile cannot be displayed.")]);}
            let id=&self.selection.names[0][..self.selection.lengths[0] as usize];let mut destination=[0;18];destination[..6].copy_from_slice(b"/APPS/");destination[6..6+id.len()].copy_from_slice(id);
            window::draw(w,&[Draw::text(40,204,0,b"Target:"),Draw::text(104,204,0,&destination[..6+id.len()])]);
            window::draw(w,&[Draw::paragraph(40,276,self.width-80,32,0x536c80,if self.mode==7 {b"Removes the app bundle from /APPS. Your documents remain."}else if self.mode==10 {b"Updates require a newer version and cannot add access. Profiles are not a complete sandbox."}else {b"Profiles limit OS services; they are not a complete security sandbox."}),Draw::label(40,320,self.width-296,0x20384d,b"Y: confirm  N / Escape: cancel")]);
        }
        if matches!(self.mode,6|7|10) {
            for (i,button) in self.review_buttons.iter().enumerate() {
                let mut button=*button;button.rect=xinyi::widgets::Rect::new(self.width as i32-if i==0 {232}else{128},312,88,24);
                xinyi::gui::button_flat(w,&button,if i==1 {b"Cancel"}else if self.mode==6 {b"Install"}else if self.mode==10 {b"Update"}else{b"Remove"},i==0);
            }
        }
        if let Some((x, y)) = self.menu {
            window::draw(
                w,
                &[
                    Draw::rect((x + 4) as u32, (y + 4) as u32, 164, 246, 0x404040),
                    Draw::rect(x as u32, y as u32, 168, 250, 0x94aabb),Draw::rect((x+1) as u32,(y+1) as u32,166,248,0xf3f6f8),
                ],
            );
            for (i, label) in [
                b"Open" as &[u8],
                b"Rename",
                b"Copy",
                b"Move",
                b"Paste",
                b"Recycle",
                b"Refresh",
                b"Install app",
                b"Remove app",
                b"Properties",
                b"Update app",
            ]
            .iter()
            .enumerate()
            {
                let yy = y + 4 + i as i32 * 22;
                let hot = self.hover == Some(i) && self.enabled(i);
                window::draw(
                    w,
                    &[
                        Draw::rect(
                            (x + 4) as u32,
                            yy as u32,
                            160,
                            22,
                            if hot { 0x2b5268 } else { 0xf3f6f8 },
                        ),
                        Draw::text(
                            (x + 10) as u32,
                            (yy + 3) as u32,
                            if !self.enabled(i) {
                                0x707070
                            } else if hot {
                                0xffffff
                            } else {
                                0
                            },
                            label,
                        ),
                    ],
                );
            }
        }
        if self.mode == 9 {
            window::draw(w,&[
                Draw::rect(24,104,self.width-48,248,0xe6edf2),Draw::text(40,120,0,b"Favourite folders - Enter or click to open"),
                Draw::text(40,328,0,b"Ctrl+D in a folder adds/removes it. Escape closes Places."),
            ]);
            if self.favorites.count==0 {window::draw(w,&[Draw::text(40,152,0,b"No favourites yet. Close Places and press Ctrl+D.")]);}
            for i in 0..self.favorites.count {
                let n=self.favorites.lengths[i] as usize;let selected=i==self.favorite_focus;
                window::draw(w,&[Draw::rect(32,144+i as u32*22,self.width-64,22,if selected {0x2b5268}else{0xffffff}),Draw::text(40,147+i as u32*22,if selected {0xffffff}else{0},&self.favorites.paths[i][n.saturating_sub(68)..n])]);
            }
        }
        if self.mode == 8 {
            let name = &self.selection.names[0][..self.selection.lengths[0] as usize];
            let size = self.property.map_or(0, |e| e.size);
            let mut digits = [b'0'; 10]; let mut value = size;
            for d in digits.iter_mut().rev() { *d += (value % 10) as u8; value /= 10; }
            let start = digits.iter().position(|&b| b != b'0').unwrap_or(9);
            window::draw(w, &[
                Draw::rect(24, 104, self.width - 48, 224, 0xe6edf2),
                Draw::text(40, 120, 0, b"Properties"),
                Draw::text(40, 152, 0, name),
                Draw::text(40, 184, 0, if self.selection.flags[0] & 1 != 0 { b"Folder" } else { b"File size (bytes):" }),
                Draw::text(208, 184, 0, if self.selection.flags[0] & 1 != 0 { b"Not calculated" } else { &digits[start..] }),
                Draw::text(40, 216, 0, b"Location:"),
                Draw::text(40, 240, 0, &self.path[..self.length.min(68)]),
                Draw::text(40, 256, 0, &self.path[self.length.min(68)..self.length]),
                Draw::text(40, 296, 0, b"Enter / Escape / click to close"),
            ]);
        }
        window::present(w);
    }
}
fn main() -> u32 {
    let w = window::open(b"Files");
    if w < 0 {
        return 1;
    }
    let w = w as u32;
    let mut app = Files {
        review_buttons:[xinyi::widgets::Button::new(xinyi::widgets::Rect::new(408,312,88,24)),xinyi::widgets::Button::new(xinyi::widgets::Rect::new(512,312,88,24))],review:None,toolbar_hot:None,handle:w, view:View::Details, favorites:Favorites::new(), favorite_focus:0, transfer_total:0, transfer_done:0,
        width: 640,
        height: 400,
        list: Listing::new(),
        sort: Sort::default(), cached: 0, history: History::new(),
        name_end: 320, type_end: 536, column_drag: None, property: None,
        offset: 0,
        selection: Selection::new(),
        path: [0; 105],
        length: 0,
        volume: 0,
        status: b"Ctrl/Shift selects multiple items. Right-click for actions.",
        mode: 0,
        input: [0; 12],
        used: 0,
        last: usize::MAX,
        clicked: 0,
        modifiers: 0,
        menu: None,
        hover: None,
        can_paste: false,
        pending: FileClip::new(),
        quit: false,
        press: None,
        dragging: false,
        open_on_release: false,
        drop_batch: false,
        destination: [0; 105],
        destination_len: 0,
        drop_row: None,
    };
    window::resizable(w, 640, 400, 1280, 1024);
    let mut args=[0;128];let argc=xinyi::arguments(&mut args);
    let mut paste_version=0;let mut recycle=false;
    if argc>0 {
        let mut raw=&args[..argc as usize];
        if raw.starts_with(b"--paste ") {
            if raw.len()<20 {return 1;}
            paste_version=match core::str::from_utf8(&raw[8..18]).ok().and_then(|s|s.parse::<u32>().ok()){Some(v) if v>0=>v,_=>return 1};
            if raw[18]!=b' ' {return 1;}raw=&raw[19..];
        }
        if raw.starts_with(b"--recycle ") {recycle=true;raw=&raw[10..];}
        let raw=raw;let folder=raw.starts_with(b"--folder ")||paste_version!=0||recycle;let path=if raw.starts_with(b"--folder ") {&raw[9..]}else{raw};
        if directory::cd(path)<0 {
            if folder {return 1;}
            if let Some(at)=path.iter().rposition(|b|*b==b'/') {
                if directory::cd(&path[..at.max(1)])==0 {if xinyi::associations::open(path,directory::volume())==0 {return 0;}}
            }
        }
    }
    app.refresh();
    app.history.record(&app.path[..app.length]);
    if app.rows() > 0 {
        app.select(0, false, false);
    }
    if paste_version!=0 {app.paste_checked(paste_version);}
    app.paint(w);
    xinyi::drag_drop::register();
    let mut buttons = 0;
    loop {
        let mut e = window::Event::default();
        let n = window::event(w, &mut e, true);
        if n < 0 {
            return 1;
        }
        if n == 0 {
            continue;
        }
        let repaint = match e.kind {
            window::DROP => {
                buttons = 0;
                if recycle {recycle=false;app.receive_recycle(e);}else{app.receive_drop(e);}
                true
            }
            window::DRAG_OVER => {
                app.drop_row = app.item_at(e.x,e.y).filter(|&row|app.list.entries[row].flags&1!=0);
                true
            }
            window::DRAG_END => {
                buttons = 0;
                app.press = None;
                app.dragging = false;
                app.open_on_release = false;
                app.drop_row = None;
                app.refresh();
                true
            }
            window::RESIZE => {
                app.resize(w, e);
                true
            }
            window::CLOSE => return 0,
            window::KEY => {
                if app.key(e.code) {
                    return 0;
                }
                true
            }
            window::RESET => {
                buttons = e.buttons;
                app.menu = None;
                app.press = None;
                app.dragging = false;
                app.open_on_release = false;
                app.drop_row = None;
                app.modifiers = 0;
                true
            }
            window::POINTER => {
                let changed = app.pointer(e, buttons);
                buttons = e.buttons;
                changed
            }
            _ => false,
        };
        if app.quit {
            return 0;
        }
        if repaint {
            app.paint(w);
        }
    }
}
