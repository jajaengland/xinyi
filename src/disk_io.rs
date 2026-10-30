// One serialized filesystem worker. It yields only after a device has finished
// a complete block/readback, never while a controller or BIOS thunk is active.
use crate::loader::Frames;
use core::arch::asm;

#[derive(Clone, Copy)]
pub struct Request {
    pub number: u32,
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub volume: u32,
}

pub fn handles(number: u32, a: u32) -> bool {
    matches!(number, 57 | 58 | 78..=81 | 88..=94 | 102) || number == 84 && a == 2
}

static mut ACTIVE: bool = false;
static mut EXECUTING: bool = false;
static mut OWNER: u32 = 0;
static mut REQUEST: Request = Request { number: 0, a: 0, b: 0, c: 0, volume: 0 };
static mut HOST_SP: u32 = 0;
static mut WORKER_SP: u32 = 0;
static mut BLOCKS: u32 = 0;
static mut SLICE: u32 = 0;
static mut DONE: bool = false;
static mut RESULT: u32 = 0;
static mut LOADING:bool=false;
static mut CANCEL_LOAD:bool=false;
static mut LOAD_PATH:[u8;105]=[0;105];
static mut LOAD_LENGTH:usize=0;
static mut LOADED:Option<Result<crate::loader::LoadedProgram,&'static str>>=None;

unsafe extern "C" {
    fn xinyi_io_switch(save: *mut u32, load: u32);
}

pub fn busy() -> bool { unsafe { ACTIVE } }
pub fn owner() -> Option<u32> { unsafe { if ACTIVE && !LOADING { Some(OWNER) } else { None } } }
pub fn blocks() -> u32 { unsafe { BLOCKS } }
pub fn loading()->bool {unsafe {ACTIVE&&LOADING}}
pub fn cancel_load()->bool {
    if !loading() {return false;}
    unsafe {CANCEL_LOAD=true;}
    crate::debug::write_str("App load: cancel requested\n");true
}
pub fn cancel_requested()->bool {unsafe {LOADING&&CANCEL_LOAD}}
pub fn load_cancelled()->bool {unsafe {EXECUTING&&LOADING&&CANCEL_LOAD}}
pub fn take_loaded()->Result<crate::loader::LoadedProgram,&'static str> {
    assert!(!busy());
    unsafe {(*core::ptr::addr_of_mut!(LOADED)).take().expect("missing app load result")}
}

pub struct Worker { stack: Frames }
impl Worker {
    pub fn new() -> Option<Self> { Some(Self { stack: Frames::new(32)? }) }

    pub fn start(&mut self, pid: u32, request: Request) {
        assert!(!busy());
        assert!(unsafe {(*core::ptr::addr_of!(LOADED)).is_none()});
        unsafe {
            LOADING=false;CANCEL_LOAD=false;
            core::slice::from_raw_parts_mut(self.stack.base() as *mut u32, 16).fill(0x494f5354);
            // The switch restores EDI/ESI/EBX/EBP/EFLAGS then RET. At entry the
            // i386 ABI requires ESP % 16 == 12, with a dummy return address.
            let frame = (self.stack.top() - 28) as *mut u32;
            for (index, value) in [0, 0, 0, 0, 0x202, entry as *const () as u32, 0].iter().enumerate() {
                frame.add(index).write(*value);
            }
            WORKER_SP = frame as u32;
            OWNER = pid;
            REQUEST = request;
            BLOCKS = 0;
            DONE = false;
            ACTIVE = true;
        }
        if pid!=0 {crate::debug::write_fmt(format_args!("Disk I/O: start pid={} nr={}\n", pid, request.number));}
    }
    pub fn start_load(&mut self,path:&str,volume:u32) {
        assert!(path.len()<=105);
        self.start(0,Request{number:0,a:0,b:0,c:0,volume});
        unsafe {
            LOADING=true;LOAD_LENGTH=path.len();
            (&mut *core::ptr::addr_of_mut!(LOAD_PATH))[..path.len()].copy_from_slice(path.as_bytes());
        }
        crate::debug::write_fmt(format_args!("App load: start path={} volume={}\n",path,volume));
    }

    // Caller installs the owning process's immutable address space and cwd.
    pub fn step(&mut self) -> Option<u32> {
        assert!(busy());
        unsafe {
            assert!(!DONE);
            SLICE = 0;
            EXECUTING = true;
            xinyi_io_switch(core::ptr::addr_of_mut!(HOST_SP), WORKER_SP);
            EXECUTING = false;
            assert!(core::slice::from_raw_parts(self.stack.base() as *const u32, 16)
                .iter().all(|&word| word == 0x494f5354), "disk worker stack overflow");
            if DONE {
                let result = RESULT;
                let (owner, blocks) = (OWNER, BLOCKS);
                if LOADING {
                    crate::debug::write_fmt(format_args!("App load: done blocks={} result={} cancelled={}\n",blocks,result as i32,{CANCEL_LOAD}));
                } else {
                    crate::debug::write_fmt(format_args!("Disk I/O: done pid={} blocks={} result={}\n", owner, blocks, result as i32));
                }
                ACTIVE = false;
                OWNER = 0;
                Some(result)
            } else { None }
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) { assert!(!busy(), "cannot free a suspended filesystem stack"); }
}

extern "C" fn entry() -> ! {
    let request = unsafe { REQUEST };
    let result = if unsafe {LOADING} {
        let (path,length)=unsafe {(LOAD_PATH,LOAD_LENGTH)};
        let result=if request.volume!=crate::diskfs::volume_epoch() {
            Err("App source volume changed; launch again.")
        } else {crate::loader::from_disk(core::str::from_utf8(&path[..length]).unwrap())};
        let result=if cancel_requested() {drop(result);Err("App load cancelled.")} else {result};
        let code=if result.is_ok() {0} else {(-5i32) as u32};
        unsafe {LOADED=Some(result);}code
    } else if !crate::usermode::profile().allows(request.number,request.a) {
        (-13i32) as u32
    } else if request.volume != crate::diskfs::volume_epoch() {
        (-116i32) as u32
    } else { match request.number {
        57 | 58 => crate::files_api::syscall(0x1b, request.number, request.a, request.b, request.c),
        78..=81 => crate::directory_api::syscall(0x1b, request.number, request.a, request.b, request.c),
        84 => crate::theme::syscall(0x1b, request.a, request.b, request.c),
        91..=94 => crate::files_api::stream(0x1b, request.number, request.a, request.b, request.c),
        88..=90 => crate::files_api::large(0x1b, request.number, request.a, request.b, request.c),
        102 => crate::app_install::syscall(0x1b,request.a,request.b,request.c),
        _ => (-38i32) as u32,
    }};
    unsafe { RESULT = result; DONE = true; }
    loop { unsafe { xinyi_io_switch(core::ptr::addr_of_mut!(WORKER_SP), HOST_SP); } }
}

pub fn checkpoint() {
    if !unsafe { EXECUTING } { return; }
    let flags: u32;
    unsafe {
        asm!("pushfd", "pop {0}", "cli", out(reg) flags);
        BLOCKS = BLOCKS.saturating_add(1);
        SLICE += 1;
        if SLICE >= 8 {
            EXECUTING = false;
            xinyi_io_switch(core::ptr::addr_of_mut!(WORKER_SP), HOST_SP);
            EXECUTING = true;
        }
        if flags & 0x200 != 0 { asm!("sti", options(nostack)); }
    }
}
