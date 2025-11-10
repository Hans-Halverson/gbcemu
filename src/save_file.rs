use std::{array, fs};

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::cartridge::Cartridge;

/// The file extension for our custom save file format.
pub const SAVE_FILE_EXTENSION: &str = ".svgb";

/// Automatically flush the save file to disk every 5 seconds.
pub const SAVE_FILE_AUTO_FLUSH_INTERVAL_SECS: u64 = 5;

pub const NUM_QUICK_SAVE_SLOTS: usize = 10;

/// A save file for a ROM. Includes both the saved data on the cartridge as well as the save states
/// for this ROM.
#[derive(Serialize, Deserialize)]
pub struct SaveFile {
    /// The serialized state of the cartridge. Only cartridge state.
    #[serde(with = "serde_bytes")]
    pub cartridge: Vec<u8>,

    /// The serialized state of the last quick save. Includes the state for the entire emulator.
    pub quick_saves: [Option<ByteBuf>; NUM_QUICK_SAVE_SLOTS],
}

impl SaveFile {
    pub fn new(cartridge: &Cartridge) -> Self {
        let cartridge_bytes = rmp_serde::to_vec(cartridge).unwrap();

        SaveFile {
            cartridge: cartridge_bytes,
            quick_saves: array::from_fn(|_| None),
        }
    }

    pub fn update_cartridge_state(&mut self, cartridge: &Cartridge) {
        let cartridge_bytes = rmp_serde::to_vec(cartridge).unwrap();
        self.cartridge = cartridge_bytes;
    }

    pub fn flush_to_disk(&self, path: &str) {
        let save_file_bytes = rmp_serde::to_vec(self).unwrap();
        fs::write(path, save_file_bytes).unwrap();
    }
}
