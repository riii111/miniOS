use crate::mutex::Mutex;
use core::mem::size_of;
use core::ptr::read_volatile;
use core::ptr::write_volatile;
use core::time::Duration;

const TIMER_CONFIG_LEVEL_TRIGGER: u64 = 1 << 1;
const TIMER_CONFIG_INT_ENABLE: u64 = 1 << 2;
const TIMER_CONFIG_USE_PERIODIC_MODE: u64 = 1 << 3;

#[repr(C)]
struct TimerRegister {
    configuration_and_capability: u64,
    _reserved: [u64; 3],
}
const _: () = assert!(size_of::<TimerRegister>() == 0x20);
impl TimerRegister {
    unsafe fn write_config(&mut self, config: u64) {
        write_volatile(&mut self.configuration_and_capability, config);
    }
}

#[repr(C)]
pub struct HpetRegisters {
    capabilities_and_id: u64,
    _reserved0: u64,
    configuration: u64,
    _reserved1: [u64; 27],
    main_counter_value: u64,
    _reserved2: u64,
    timers: [TimerRegister; 32],
}
const _: () = assert!(size_of::<HpetRegisters>() == 0x500);

pub struct Hpet {
    registers: &'static mut HpetRegisters,
    #[allow(unused)]
    num_of_timers: usize,
    freq: u64,
}
impl Hpet {
    pub fn new(registers: &'static mut HpetRegisters) -> Self {
        let fs_per_count = registers.capabilities_and_id >> 32;
        let num_of_timers = ((registers.capabilities_and_id >> 8) & 0b11111) as usize + 1;
        let freq = 1_000_000_000_000_000 / fs_per_count;
        let mut hpet = Self {
            registers,
            num_of_timers,
            freq,
        };
        unsafe {
            hpet.globally_disable();
            for i in 0..hpet.num_of_timers {
                let timer = &mut hpet.registers.timers[i];
                let mut config = read_volatile(&timer.configuration_and_capability);
                config &= !(TIMER_CONFIG_INT_ENABLE
                    | TIMER_CONFIG_USE_PERIODIC_MODE
                    | TIMER_CONFIG_LEVEL_TRIGGER
                    | (0b11111 << 9));
                timer.write_config(config);
            }
            write_volatile(&mut hpet.registers.main_counter_value, 0);
            hpet.globally_enable();
        }
        hpet
    }
    unsafe fn globally_disable(&mut self) {
        let config = read_volatile(&self.registers.configuration) & !0b11;
        write_volatile(&mut self.registers.configuration, config);
    }
    unsafe fn globally_enable(&mut self) {
        let config = read_volatile(&self.registers.configuration) | 0b01;
        write_volatile(&mut self.registers.configuration, config);
    }
    pub fn main_counter(&self) -> u64 {
        unsafe { read_volatile(&self.registers.main_counter_value) }
    }
    pub fn freq(&self) -> u64 {
        self.freq
    }
}
static HPET: Mutex<Option<Hpet>> = Mutex::new(None);
pub fn set_global_hpet(hpet: Hpet) {
    assert!(HPET.lock().is_none());
    *HPET.lock() = Some(hpet);
}
pub fn global_timestamp() -> Duration {
    if let Some(hpet) = &*HPET.lock() {
        let ns = hpet.main_counter() as u128 * 1_000_000_000 / hpet.freq() as u128;
        Duration::from_nanos(ns as u64)
    } else {
        Duration::ZERO
    }
}
