// Fixed prefix of isr128's PUSHA + vector/error + CPU interrupt frame.
// User ESP/SS follow only when the interrupt crosses privilege levels.
#[repr(C)]
pub struct SyscallFrame {
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
    pub eflags: u32,
}

// Keep the Rust/assembly boundary checked in every build profile.
const _: () = {
    assert!(core::mem::size_of::<SyscallFrame>() == 52);
    assert!(core::mem::offset_of!(SyscallFrame, ebx) == 16);
    assert!(core::mem::offset_of!(SyscallFrame, edx) == 20);
    assert!(core::mem::offset_of!(SyscallFrame, ecx) == 24);
    assert!(core::mem::offset_of!(SyscallFrame, eax) == 28);
    assert!(core::mem::offset_of!(SyscallFrame, cs) == 44);
    assert!(core::mem::offset_of!(SyscallFrame, eflags) == 48);
};
