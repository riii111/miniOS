extern crate alloc;

use crate::error;
use crate::info;
use crate::result::Result;
use alloc::boxed::Box;
use core::arch::asm;
use core::arch::global_asm;
use core::fmt;
use core::marker::PhantomData;
use core::mem::offset_of;
use core::mem::size_of;
use core::mem::size_of_val;
use core::pin::Pin;

pub fn hlt() {
    unsafe { asm!("hlt") }
}

pub fn busy_loop_hint() {
    unsafe { asm!("pause") }
}

pub fn read_io_port_u8(port: u16) -> u8 {
    let mut data: u8;
    unsafe {
        asm!("in al, dx",
        out("al") data,
            in("dx") port)
    }
    data
}

pub fn write_io_port_u8(port: u16, data: u8) {
    unsafe { asm!("out dx, al", in("al") data, in("dx") port) } // no OS, so need to communicate directly with the hardware
}

pub fn read_cr3() -> *mut PML4 {
    let mut cr3: *mut PML4;
    unsafe {
        asm!("mov rax, cr3",
            out("rax") cr3)
    }
    cr3
}

pub const PAGE_SIZE: usize = 4096;
const ATTR_MASK: u64 = 0xFFF;
const ATTR_PRESENT: u64 = 1 << 0;
const ATTR_WRITABLE: u64 = 1 << 1;
const ATTR_WRITE_THROUGH: u64 = 1 << 3;
const ATTR_CACHE_DISABLE: u64 = 1 << 4;

#[derive(Debug, Copy, Clone)]
#[repr(u64)]
pub enum PageAttr {
    NotPresent = 0,
    ReadWriteKernel = ATTR_PRESENT | ATTR_WRITABLE,
    ReadWriteIo = ATTR_PRESENT | ATTR_WRITABLE | ATTR_WRITE_THROUGH | ATTR_CACHE_DISABLE,
}

#[derive(Debug, Eq, PartialEq)]
pub enum TranslationResult {
    PageMapped4K { phys: u64 },
    PageMapped2M { phys: u64 },
    PageMapped1G { phys: u64 },
}

#[repr(transparent)]
pub struct Entry<const LEVEL: usize, const SHIFT: usize, NEXT> {
    value: u64,
    next_type: PhantomData<NEXT>,
}
impl<const LEVEL: usize, const SHIFT: usize, NEXT> Entry<LEVEL, SHIFT, NEXT> {
    fn read_value(&self) -> u64 {
        self.value
    }
    fn is_present(&self) -> bool {
        (self.read_value() & (1 << 0)) != 0
    }
    fn is_writable(&self) -> bool {
        (self.read_value() & (1 << 1)) != 0
    }
    fn is_user(&self) -> bool {
        (self.read_value() & (1 << 2)) != 0
    }
    fn format(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "L{}Entry @ {:#p} {{ {:#018X} {}{}{} ",
            LEVEL,
            self,
            self.read_value(),
            if self.is_present() { "p" } else { "N" },
            if self.is_writable() { "W" } else { "R" },
            if self.is_user() { "U" } else { "S" }
        )?;
        write!(f, "}}")
    }
    fn table(&self) -> Result<&NEXT> {
        if self.is_present() {
            Ok(unsafe { &*((self.value & !ATTR_MASK) as *const NEXT) })
        } else {
            Err("Page Not Found")
        }
    }
}
impl<const LEVEL: usize, const SHIFT: usize, NEXT> fmt::Display for Entry<LEVEL, SHIFT, NEXT> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.format(f)
    }
}
impl<const LEVEL: usize, const SHIFT: usize, NEXT> fmt::Debug for Entry<LEVEL, SHIFT, NEXT> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.format(f)
    }
}

#[repr(align(4096))]
pub struct Table<const LEVEL: usize, const SHIFT: usize, NEXT> {
    entry: [Entry<LEVEL, SHIFT, NEXT>; 512],
}
impl<const LEVEL: usize, const SHIFT: usize, NEXT: core::fmt::Debug> Table<LEVEL, SHIFT, NEXT> {
    fn format(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "L{}Table @ {:#p} {{", LEVEL, self)?;
        for i in 0..512 {
            let e = &self.entry[i];
            if !e.is_present() {
                continue;
            }
            writeln!(f, "  entry[{:3}] = {:?}", i, e)?;
        }
        writeln!(f, "}}")
    }
    pub fn next_level(&self, index: usize) -> Option<&NEXT> {
        self.entry.get(index).and_then(|e| e.table().ok())
    }
}
impl<const LEVEL: usize, const SHIFT: usize, NEXT: fmt::Debug> fmt::Debug
    for Table<LEVEL, SHIFT, NEXT>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.format(f)
    }
}

pub type PT = Table<1, 12, [u8; PAGE_SIZE]>;
pub type PD = Table<2, 21, PT>;
pub type PDPT = Table<3, 30, PD>;
pub type PML4 = Table<4, 39, PDPT>;

/// # Safety
/// Anything can happen if the given selector is invalid.
pub unsafe fn write_es(selector: u16) {
    asm!(
    "mov es, ax",
                  in("ax") selector)
}

/// # Safety
/// Anything can happen if the CS given is invalid.
pub unsafe fn write_cs(cs: u16) {
    // The MOV instruction CANNOT be used to load the CS register.
    // Use far-jump(ljmp) instead.
    asm!(
  "lea rax, [rip + 2f]", // Target address (label 1 below)
  "push cx",  // Construct a far pointer on the stack
  "push rax",
  "ljmp [rsp]",
        "2:",
        "add rsp, 8 + 2",  // Cleanup the far pointer on the stack
                in("cx") cs)
}

/// # Safety
/// Anything can happen if the given selector is invalid.
pub unsafe fn write_ss(selector: u16) {
    asm!(
  "mov ss, ax",
                in("ax") selector)
}

/// # Safety
/// Anything can happen if the given selector is invalid.
pub unsafe fn write_ds(ds: u16) {
    asm!(
  "mov ds, ax",
                in("ax") ds)
}

/// # Safety
/// Anything can happen if the given selector is invalid.
pub unsafe fn write_fs(selector: u16) {
    asm!(
  "mov fs, ax",
                in("ax") selector)
}

/// # Safety
/// Anything can happen if the given selector is invalid.
pub unsafe fn write_gs(selector: u16) {
    asm!(
  "mov gs, ax",
                in("ax") selector)
}

#[allow(dead_code)]
#[repr(C)]
#[derive(Clone, Copy)]
struct FPUContenxt {
    data: [u8; 512],
}
#[allow(dead_code)]
#[repr(C)]
#[derive(Clone, Copy)]
struct GeneralRegisterContext {
    rax: u64,
    rdx: u64,
    rbx: u64,
    rbp: u64,
    rsi: u64,
    rdi: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rcx: u64,
}
const _: () = assert!(size_of::<GeneralRegisterContext>() == (16 - 1) * 8);
#[allow(dead_code)]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct InterruptContext {
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}
const _: () = assert!(size_of::<InterruptContext>() == 8 * 5);
#[allow(dead_code)]
#[repr(C)]
#[derive(Clone, Copy)]
struct InterruptInfo {
    // This struct is placed at top of the interrupt stack.
    // Should be aligned on 16-byte boundaries to pass the
    // alignment checks done by FXSAVE / FXRSTOR
    fpu_context: FPUContenxt, // used by FXSAVE / FXRSTOR
    _dummy: u64,
    greg: GeneralRegisterContext,
    error_code: u64,
    ctx: InterruptContext,
}
const _: () = assert!(size_of::<InterruptInfo>() == (16 + 4 + 1) * 8 + 8 + 512);
impl fmt::Debug for InterruptInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "
        {{
          rip: {:#018X}, CS: {:#06X},
          rsp: {:#018X}, SS: {:#06X},
          rbp: {:#018X},

          rflags:      {:#018X},
          error_code: {:#018X},

          rax: {:#018X}, rcx: {:#018X},
          rdx: {:#018X}, rbx: {:#018X},
          rsi: {:#018X}, rdi: {:#018X},
          r8:  {:#018X}, r9:  {:#018X},
          r10: {:#018X}, r11: {:#018X},
          r12: {:#018X}, r13: {:#018X},
          r14: {:#018X}, r15: {:#018X},
        }}",
            self.ctx.rip,
            self.ctx.cs,
            self.ctx.rsp,
            self.ctx.ss,
            self.greg.rbp,
            self.ctx.rflags,
            self.error_code,
            //
            self.greg.rax,
            self.greg.rcx,
            self.greg.rdx,
            self.greg.rbx,
            //
            self.greg.rsi,
            self.greg.rdi,
            //
            self.greg.r8,
            self.greg.r9,
            self.greg.r10,
            self.greg.r11,
            self.greg.r12,
            self.greg.r13,
            self.greg.r14,
            self.greg.r15,
        )
    }
}

// SDM Vol.3: 6.14.2 64-Bit Mode Stack Frame
// In IA-32e mode, the RSP is aligned to a 16-byte boundary
// before pushing the stack frame

/// This generates interrupt_entrypointN()
/// Generated asm will be looks like this:
/// ```
/// .global interrupt_entrypointN
///    interrupt_entrypointN:
///    push 0 // No error code
///    push rcx // Save rcx first to reuse
///    mov rcx, N // INT#
///    jmp inthandler_common
/// ```
macro_rules! interrupt_entrypoint {
    ($index:literal) => {
        global_asm!(concat!(
            ".global interrupt_entrypoint",
            stringify!($index),
            "\n",
            "interrupt_entrypoint",
            stringify!($index),
            ":\n",
            "push 0 // No error code\n",
            "push rcx // Save rcx first to reuse\n",
            "mov rcx, ",
            stringify!($index),
            "\n",
            "jmp inthandler_common"
        ));
    };
}

interrupt_entrypoint!(3);
interrupt_entrypoint!(6);
interrupt_entrypoint_with_ecode!(8);
interrupt_entrypoint_with_ecode!(13);
interrupt_entrypoint_with_ecode!(14);
interrupt_entrypoint!(32);

extern "sysv64" {
    fn interrupt_entrypoint3();
    fn interrupt_entrypoint6();
    fn interrupt_entrypoint8();
    fn interrupt_entrypoint13();
    fn interrupt_entrypoint14();
    fn interrupt_entrypoint32();
}

global_asm!(
    r#"
.global inthandler_common
inthandler_common:
    // General purpose registers (except rsp and rcx)
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rdi
    push rsi
    push rbp
    push rbx
    push rdx
    push rax
    // FPU State
    sub rsp, 512 + 8
    fxsave64[rsp]
    // 1st parameter: pointer to the saved CPU state
    mov rdi, rsp
    // Align the stack to 16-bytes boundary
    mov rbp, rsp
    and rsp, -16
    // 2nd parameter: Int#
    mov rsi, rcx

    call inthandler

    mov rsp, rbp
    //
    fxrstor64[rsp]
    add rsp, 512 + 8
    //
    pop rax
    pop rdx
    pop rbx
    pop rbp
    pop rsi
    pop rdi
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    //
    pop rcx
    add rsp, 8 // for Error Code
    iretq
"#
);

pub fn read_cr2() -> u64 {
    let mut cr2: u64;
    unsafe {
        asm!("mov rax, cr2",
            out("rax") cr2)
    }
    cr2
}

#[no_mangle]
extern "sysv64" fn inthandler(info: &InterruptInfo, index: usize) {
    error!("Interrupt Info: {:?}", info);
    error!("Exception {index:#04X}: ");
    match index {
        3 => {
            error!("Breakpoint");
        }
        6 => {
            error!("Invalid Opcode");
        }
        8 => {
            error!("Double Fault");
        }
        13 => {
            error!("General Protection Fault");
            let rip = info.ctx.rip;
            error!("Bytes @ RIP({rip:$018X}):");
            let rip = rip as *const u8;
            let bytes = unsafe { core::slice::from_raw_parts(rip, 16) };
            error!("  = {bytes:02X?}");
        }
        14 => {
            error!("Page Fault");
            error!("CR2={:#018X}", read_cr2());
            error!(
                "Caused by: A {} mode {} on a {} page, page structures are {}",
                if info.error_code & 0b0000_0100 != 0 {
                    "user"
                } else {
                    "supervisor"
                },
                if info.error_code & 0b0001_0000 != 0 {
                    "instruction fetch"
                } else if info.error_code & 0b0010 != 0 {
                    "data write"
                } else {
                    "data read"
                },
                if info.error_code & 0b0001 != 0 {
                    "present"
                } else {
                    "non-present"
                },
                if info.error_code & 0b1000 != 0 {
                    "invalid"
                } else {
                    "valid"
                },
            );
        }
        _ => {
            error!("Not handled");
        }
    }
    panic!("fatal exception");
}

#[no_mangle]
extern "sysv64" fn int_handler_unimplemented() {
    panic!("unexpected interrupt!");
}

// PDDRTTTT (TTTT: type, R: reserved, D: DPL, P: present)
pub const BIT_FLAGS_INTGATE: u8 = 0b0000_1110u8;
pub const BIT_FLAGS_PRESENT: u8 = 0b1000_0000u8;
pub const BIT_FLAGS_DPL0: u8 = 0 << 5;
pub const BIT_FLAGS_DPL3: u8 = 3 << 5;

#[repr(u8)]
#[derive(Copy, Clone)]
enum IdtAttr {
    // Without _NotPresent value, MaybeUninit: zeroed() on
    // this struct will be undefined behavior.
    _NotPresent = 0,
    IntGateDPL0 = BIT_FLAGS_INTGATE | BIT_FLAGS_PRESENT | BIT_FLAGS_DPL0,
    IntGateDPL3 = BIT_FLAGS_INTGATE | BIT_FLAGS_PRESENT | BIT_FLAGS_DPL3,
}

#[repr(C, packed)]
#[allow(dead_code)]
#[derive(Copy, Clone)]
pub struct IdtDescriptor {
    offset_low: u16,
    segment_selector: u16,
    ist_index: u8,
    attr: IdtAttr,
    offset_mid: u16,
    offset_high: u32,
    _reserved: u32,
}
const _: () = assert!(size_of::<IdtDescriptor>() == 16);
impl IdtDescriptor {
    fn new(
        segment_selector: u16,
        ist_index: u8,
        attr: IdtAttr,
        f: unsafe extern "sysv64" fn(),
    ) -> Self {
        let handler_addr = f as *const unsafe extern "sysv64" fn() as usize;
        Self {
            offset_low: handler_addr as u16,
            offset_mid: (handler_addr >> 16) as u16,
            offset_high: (handler_addr >> 32) as u32,
            segment_selector,
            ist_index,
            attr,
            _reserved: 0,
        }
    }
}

#[allow(dead_code)]
#[repr(C, packed)]
#[derive(Debug)]
struct IdtrParameters {
    limit: u16,
    base: *const IdtDescriptor,
}
const _: () = assert!(size_of::<IdtrParameters>() == 10);
const _: () = assert!(offset_of!(IdtrParameters, base) == 2);

pub struct Idt {
    #[allow(dead_code)]
    entries: Pin<Box<[IdtDescriptor; 0x100]>>,
}
impl Idt {
    pub fn new(segment_selector: u16) -> Self {
        let mut entries = [IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            int_handler_unimplemented,
        ); 0x100];
        entries[3] = IdtDescriptor::new(
            segment_selector,
            1,
            // Set DPL=3 to allow user land to make this interrupt (e.g. via int3 op)
            IdtAttr::IntGateDPL3,
            interrupt_entrypoint3,
        );
        entries[6] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint6,
        );
        entries[8] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint8,
        );
        entries[13] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint13,
        );
        entries[14] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint14,
        );
        entries[32] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint32,
        );
        let limit = size_of_val(&entries) as u16;
        let entries = Box::pin(entries);
        let params = IdtrParameters {
            limit,
            base: entries.as_ptr(),
        };
        info!("Loading IDT: {params:?}");
        // SAFETY: This is safe since it loads a valid IDT that is constructed
        // in the code just above
        unsafe {
            asm!("lidt [rcx]",
                in("rcx") &params);
        }
        Self { entries }
    }
}
