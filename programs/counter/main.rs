#![no_std]
#![no_main]
#[path = "../../sdk/xinyi.rs"]
mod xinyi;
use xinyi::window::{self, Draw};
fn canary(role: u8) -> bool {
    let mut value = [0u8; 8];
    let mut control = 0u16;
    let mode = if role == b'A' { 0x037fu16 } else { 0x077fu16 };
    unsafe {
        core::arch::asm!("fninit","fldcw [{0}]","fld1",in(reg)&mode,options(nostack));
        if role != b'A' {
            core::arch::asm!("fld1", "faddp st(1), st(0)", options(nostack));
        }
        // No syscalls/yields in this loop: timer preemption must preserve these
        // registers, the user stack and DF while the other program executes.
        let ok: u32;
        core::arch::asm!("push esi","push edi","push ebx","mov esi,0x13579bdf","mov edi,0x2468ace0","mov ebx,0x5a5aa5a5","mov ecx,30000000","std","2:","dec ecx","jnz 2b","pushfd","pop eax","and eax,0x400","cmp esi,0x13579bdf","jne 3f","cmp edi,0x2468ace0","jne 3f","cmp ebx,0x5a5aa5a5","jne 3f","jmp 4f","3:","xor eax,eax","4:","cld","pop ebx","pop edi","pop esi",out("eax")ok,out("ecx")_);
        core::arch::asm!("fstp qword ptr [{0}]","fnstcw [{1}]",in(reg)value.as_mut_ptr(),in(reg)&mut control,options(nostack));
        ok == 0x400
            && control == mode
            && u64::from_le_bytes(value)
                == if role == b'A' {
                    0x3ff0000000000000
                } else {
                    0x4000000000000000
                }
    }
}
fn number(line: &mut [u8], at: usize, value: u32) {
    let mut v = value;
    let mut digits = [b'0'; 10];
    for d in digits.iter_mut().rev() {
        *d = b'0' + (v % 10) as u8;
        v /= 10;
    }
    line[at..at + 10].copy_from_slice(&digits);
}
fn main() -> u32 {
    let mut args = [0; 128];
    let n = xinyi::arguments(&mut args).max(0) as usize;
    let role = if args.first() == Some(&b'B') {
        b'B'
    } else {
        b'A'
    };
    let mode = if n > 2 { &args[2..n] } else { b"" };
    let mut title = *b"Counter A";
    title[8] = role;
    let handle = window::open_sized(&title, 320, 144);
    if handle < 0 {
        xinyi::print("Cannot open counter window.\n");
        return 1;
    }
    let handle = handle as u32;
    let started = xinyi::ticks();
    let mut last = started.wrapping_sub(10);
    loop {
        let now = xinyi::ticks();
        let count = now.wrapping_sub(started) / 10;
        if now.wrapping_sub(last) >= 10 {
            let mut line = [b' '; 38];
            line[..7].copy_from_slice(b"Count: ");
            number(&mut line, 7, count);
            let mut pid = [b' '; 38];
            pid[..5].copy_from_slice(b"PID: ");
            number(&mut pid, 5, xinyi::getpid());
            let color = if role == b'A' { 0x187c88 } else { 0x7755aa };
            if window::draw(
                handle,
                &[
                    Draw::rect(0, 0, 320, 144, 0xf4f4f4),
                    Draw::text(8, 12, 0, b"Independent user program"),
                    Draw::text(8, 40, 0, &line),
                    Draw::text(8, 64, 0, &pid),
                    Draw::rect(8, 92, 304, 18, 0xdddddd),
                    Draw::rect(8, 92, ((count % 76) + 1) * 4, 18, color),
                    Draw::text(8, 120, 0, b"Q quits | C crashes this app"),
                ],
            ) < 0
            {
                return 1;
            }
            window::present(handle);
            last = now;
        }
        if mode == b"check" {
            return if canary(role) { 0 } else { 91 };
        }
        if mode == b"hog" {
            loop {
                unsafe {
                    core::arch::asm!("pause", options(nomem, nostack));
                }
            }
        }
        if mode == b"fault" && count >= 5 {
            unsafe {
                core::arch::asm!("ud2", options(noreturn));
            }
        }
        let mut event = window::Event::default();
        let result = window::event(handle, &mut event, mode == b"wait");
        if result < 0 {
            return 1;
        }
        if result > 0 {
            if event.kind == window::CLOSE {
                return 0;
            }
            if event.kind == window::KEY {
                match event.code as u16 {
                    81 | 113 => return 0,
                    67 | 99 => unsafe {
                        core::arch::asm!("ud2", options(noreturn));
                    },
                    _ => (),
                }
            }
        }
        xinyi::sleep_ms(20);
    }
}
