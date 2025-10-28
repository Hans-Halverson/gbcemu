use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    address_space::{
        Address, EXTERNAL_RAM_START, FIRST_ROM_BANK_END, ROM_BANK_SIZE,
        SINGLE_EXTERNAL_RAM_BANK_SIZE,
    },
    mbc::mbc::{Location, Mbc, MbcKind, RegisterHandle},
};

pub struct Mbc3 {
    /// RAM & RTC Enable Register (0000–2000)
    is_ram_rtc_enabled: bool,
    /// ROM Bank Number, 7 bits (2000–4000)
    rom_bank_num: u8,
    /// RAM Bank Number or RTC register (4000–6000)
    ram_rtc_mapping: RamRtcMapping,
    /// Saved time value
    latched_clock_time: Option<SystemTime>,
    /// The last value written to the latch clock data register.
    /// Used to detect rising edge from 0x00 to 0x01.
    last_latched_write: Option<u8>,
}

enum RtcRegister {
    Seconds,
    Minutes,
    Hours,
    DayLow,
    DayHigh,
}

enum RamRtcMapping {
    RamBank(u8),
    RtcRegister(RtcRegister),
}

impl Mbc3 {
    pub fn new() -> Self {
        Mbc3 {
            is_ram_rtc_enabled: false,
            rom_bank_num: 1,
            ram_rtc_mapping: RamRtcMapping::RamBank(0),
            latched_clock_time: None,
            last_latched_write: None,
        }
    }
}

const RAM_RTC_ENABLE_REGISTER: RegisterHandle = 0;
const ROM_BANK_NUMBER_REGISTER: RegisterHandle = 1;
const RAM_RTC_MAPPING_REGISTER: RegisterHandle = 2;
const LATCH_CLOCK_DATA_REGISTER: RegisterHandle = 3;
const RTC_REGISTER_SECONDS: RegisterHandle = 4;
const RTC_REGISTER_MINUTES: RegisterHandle = 5;
const RTC_REGISTER_HOURS: RegisterHandle = 6;
const RTC_REGISTER_DAY_LOW: RegisterHandle = 7;
const RTC_REGISTER_DAY_HIGH: RegisterHandle = 8;

/// Treat reads or writes to uninitialized RAM value register as reading/writing from a register
/// that always returns 0xFF/is ignored respectively.
const UNITIALIZED_RAM_VALUE_REGISTER: RegisterHandle = 9;

impl Mbc3 {
    /// Address expected to be in the range 0xA000-0xC000
    fn physical_ram_bank_address(bank_num: usize, addr: Address) -> usize {
        let physical_bank_start_offset = bank_num * SINGLE_EXTERNAL_RAM_BANK_SIZE;
        let offset_in_bank = (addr - EXTERNAL_RAM_START) as usize;
        let physical_addr = physical_bank_start_offset + offset_in_bank;

        physical_addr
    }

    fn map_ram_address(&self, addr: Address) -> Location {
        if !self.is_ram_rtc_enabled {
            return Location::Register(UNITIALIZED_RAM_VALUE_REGISTER);
        }

        match self.ram_rtc_mapping {
            RamRtcMapping::RamBank(bank_num) => {
                return Location::Address(Self::physical_ram_bank_address(bank_num as usize, addr));
            }
            RamRtcMapping::RtcRegister(RtcRegister::Seconds) => {
                return Location::Register(RTC_REGISTER_SECONDS);
            }
            RamRtcMapping::RtcRegister(RtcRegister::Minutes) => {
                return Location::Register(RTC_REGISTER_MINUTES);
            }
            RamRtcMapping::RtcRegister(RtcRegister::Hours) => {
                return Location::Register(RTC_REGISTER_HOURS);
            }
            RamRtcMapping::RtcRegister(RtcRegister::DayLow) => {
                return Location::Register(RTC_REGISTER_DAY_LOW);
            }
            RamRtcMapping::RtcRegister(RtcRegister::DayHigh) => {
                return Location::Register(RTC_REGISTER_DAY_HIGH);
            }
        }
    }
}

impl Mbc for Mbc3 {
    fn kind(&self) -> MbcKind {
        MbcKind::Mbc3
    }

    fn map_read_rom_address(&self, addr: Address) -> usize {
        if addr < FIRST_ROM_BANK_END {
            addr as usize
        } else {
            addr as usize + ((self.rom_bank_num as usize - 1) * ROM_BANK_SIZE as usize)
        }
    }

    fn map_write_rom_address(&self, addr: Address) -> Location {
        match addr {
            0..0x2000 => Location::Register(RAM_RTC_ENABLE_REGISTER),
            0x2000..0x4000 => Location::Register(ROM_BANK_NUMBER_REGISTER),
            0x4000..0x6000 => Location::Register(RAM_RTC_MAPPING_REGISTER),
            0x6000..0x8000 => Location::Register(LATCH_CLOCK_DATA_REGISTER),
            _ => unreachable!(),
        }
    }

    fn map_read_ram_address(&self, addr: Address) -> Location {
        self.map_ram_address(addr)
    }

    fn map_write_ram_address(&self, addr: Address) -> Location {
        self.map_ram_address(addr)
    }

    fn read_register(&self, reg: RegisterHandle) -> u8 {
        match reg {
            // RAM always returns 0xFF until initialized
            UNITIALIZED_RAM_VALUE_REGISTER => 0xFF,
            // Calculate current number of seconds in the minute from RTC
            RTC_REGISTER_SECONDS => {
                if let Some(time) = &self.latched_clock_time {
                    (time.duration_since(UNIX_EPOCH).unwrap().as_secs() % 60) as u8
                } else {
                    0
                }
            }
            // Calculate current number of minutes in the hour from RTC
            RTC_REGISTER_MINUTES => {
                if let Some(time) = &self.latched_clock_time {
                    ((time.duration_since(UNIX_EPOCH).unwrap().as_secs() / 60) % 60) as u8
                } else {
                    0
                }
            }
            // Calculate current number of hours in the day from RTC
            RTC_REGISTER_HOURS => {
                if let Some(time) = &self.latched_clock_time {
                    ((time.duration_since(UNIX_EPOCH).unwrap().as_secs() / 3600) % 24) as u8
                } else {
                    0
                }
            }
            // Low 8 bits of the (9 bit) day counter from RTC
            RTC_REGISTER_DAY_LOW => {
                if let Some(time) = &self.latched_clock_time {
                    ((time.duration_since(UNIX_EPOCH).unwrap().as_secs() / 86400) & 0xFF) as u8
                } else {
                    0
                }
            }
            // High bit of the day counter from RTC
            // TODO: Implement halt and carry bits
            RTC_REGISTER_DAY_HIGH => {
                if let Some(time) = &self.latched_clock_time {
                    let days = (time.duration_since(UNIX_EPOCH).unwrap().as_secs() / 86400) as u16;
                    let day_high = ((days >> 8) & 0x1) as u8;
                    day_high
                } else {
                    0
                }
            }
            _ => unreachable!(),
        }
    }

    fn write_register(&mut self, register: RegisterHandle, value: u8) {
        match register {
            // RAM is enabled by setting the lower nibble to 0xA, otherwise is disabled
            RAM_RTC_ENABLE_REGISTER => {
                self.is_ram_rtc_enabled = (value & 0xF) == 0xA;
            }
            // Only 7 bits of the value are used. Enforce that bank number 0 is remapped to 1 when
            // written.
            ROM_BANK_NUMBER_REGISTER => {
                let mut bank_num = value & 0x7F;
                if bank_num == 0 {
                    bank_num = 1;
                }
                self.rom_bank_num = bank_num;
            }
            // Either enable RAM or RTC
            RAM_RTC_MAPPING_REGISTER => {
                let mapping = match value {
                    0x0..0x8 => RamRtcMapping::RamBank(value),
                    0x8 => RamRtcMapping::RtcRegister(RtcRegister::Seconds),
                    0x9 => RamRtcMapping::RtcRegister(RtcRegister::Minutes),
                    0xA => RamRtcMapping::RtcRegister(RtcRegister::Hours),
                    0xB => RamRtcMapping::RtcRegister(RtcRegister::DayLow),
                    0xC => RamRtcMapping::RtcRegister(RtcRegister::DayHigh),
                    _ => panic!("Invalid RAM/RTC register value written: 0x{:02X}", value),
                };

                self.ram_rtc_mapping = mapping;
            }
            // A write of 0x00 followed by a write of 0x01 latches the current time into the RTC
            LATCH_CLOCK_DATA_REGISTER => {
                if value == 0 {
                    self.last_latched_write = Some(0);
                    return;
                }

                if self.last_latched_write == Some(0) && value == 1 {
                    self.latched_clock_time = Some(SystemTime::now());
                    self.last_latched_write = None;
                    return;
                }

                self.last_latched_write = None;
            }
            // Writes to unitialized RAM are modeled as a write to a register that is ignored
            UNITIALIZED_RAM_VALUE_REGISTER => {}
            // Ignore writes to RTC register for now
            // TODO: Implement writable RTC registers
            RTC_REGISTER_SECONDS
            | RTC_REGISTER_MINUTES
            | RTC_REGISTER_HOURS
            | RTC_REGISTER_DAY_LOW
            | RTC_REGISTER_DAY_HIGH => {}
            _ => unreachable!(),
        }
    }
}
