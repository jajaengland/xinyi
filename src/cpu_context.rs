// Same layout as the segment saves + PUSHA/vector/error/IRET frame.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Cpu {
    pub gs: u32,
    pub fs: u32,
    pub es: u32,
    pub ds: u32,
    pub edi: u32,
    pub esi: u32,
    pub ebp: u32,
    pub saved_esp: u32,
    pub ebx: u32,
    pub edx: u32,
    pub ecx: u32,
    pub eax: u32,
    pub vector: u32,
    pub error: u32,
    pub eip: u32,
    pub cs: u32,
    pub flags: u32,
    pub esp: u32,
    pub ss: u32,
}
impl Cpu {
    pub fn new() -> Self {
        Self {
            gs: 0x23,
            fs: 0x23,
            es: 0x23,
            ds: 0x23,
            eip: crate::paging::USER_BASE,
            cs: 0x1b,
            flags: 0x202,
            esp: crate::paging::USER_STACK_TOP - 4,
            ss: 0x23,
            ..Self::default()
        }
    }
    pub unsafe fn capture(
        frame: &crate::syscall_frame::SyscallFrame,
        segments: *const u32,
    ) -> Self {
        let raw = frame as *const _ as *const u32;
        Self {
            gs: segments.read() & 0xffff,
            fs: segments.add(1).read() & 0xffff,
            es: segments.add(2).read() & 0xffff,
            ds: segments.add(3).read() & 0xffff,
            edi: frame.edi,
            esi: frame.esi,
            ebp: frame.ebp,
            saved_esp: 0,
            ebx: frame.ebx,
            edx: frame.edx,
            ecx: frame.ecx,
            eax: frame.eax,
            vector: frame.vector,
            error: frame.error,
            eip: frame.eip,
            cs: frame.cs,
            flags: frame.eflags,
            esp: raw.add(13).read(),
            ss: raw.add(14).read(),
        }
    }
}
const _: () = assert!(
    core::mem::size_of::<Cpu>() == 76
        && core::mem::offset_of!(Cpu, eip) == 56
        && core::mem::offset_of!(Cpu, flags) == 64
        && core::mem::offset_of!(Cpu, esp) == 68
        && core::mem::offset_of!(Cpu, ss) == 72
);
