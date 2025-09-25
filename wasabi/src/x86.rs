use core::arch::asm;

pub fn hlt() {
    unsafe { asm!("hlt") }
}

pub fn write_io_part_u8(port: i16, data: u8) {
    unsafe { asm!("out dx, al", in("al") data, in("dx") port) }
}
