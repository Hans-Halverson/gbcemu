use serde::{Deserialize, Serialize};

use crate::address_space::SINGLE_VRAM_BANK_SIZE;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Machine {
    /// The original GameBoy
    Dmg,
    /// The GameBoy Color
    Cgb,
}

impl Machine {
    pub const fn vram_size(&self) -> usize {
        match self {
            Machine::Dmg => 1 * SINGLE_VRAM_BANK_SIZE,
            Machine::Cgb => 2 * SINGLE_VRAM_BANK_SIZE,
        }
    }
}
