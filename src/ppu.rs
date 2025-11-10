use std::{fmt::Debug, mem};

use serde::{Deserialize, Serialize};

use crate::emulator::{CgbPaletteData, Emulator, SCREEN_WIDTH};

/// A sprite in OAM.
struct Object {
    y: u8,
    x: u8,
    tile_index: u8,
    attributes: u8,
}

impl Object {
    fn cgb_pallette_number(&self) -> usize {
        (self.attributes & 0x07) as usize
    }

    fn vram_bank_number(&self) -> usize {
        ((self.attributes & 0x08) >> 3) as usize
    }

    /// Whether the palette is OBP0 or OBP1
    fn dmg_palette_number(&self) -> u8 {
        (self.attributes & 0x10) >> 4
    }

    fn is_horizontally_flipped(&self) -> bool {
        self.attributes & 0x20 != 0
    }

    fn is_vertically_flipped(&self) -> bool {
        self.attributes & 0x40 != 0
    }

    /// Non-CGB mode:
    ///
    /// If false, object has priority to be drawn on top of background (and window) if not transparent.
    /// If true, object does not have priority and is drawn behind background (and window) unless
    /// background color is transparent.
    ///
    /// CGB mode:
    ///
    /// If true, object has priority to be drawn behind background.
    /// - LCDC priority flag overrides this
    /// - This is overridden by background tile's priority flag
    fn in_background(&self) -> bool {
        self.attributes & 0x80 != 0
    }
}

/// Attributes for a background tile (CGB mode only).
struct BackgroundTileAttributes {
    raw: u8,
}

impl BackgroundTileAttributes {
    fn color_palette(&self) -> usize {
        (self.raw & 0x07) as usize
    }

    fn vram_bank_number(&self) -> usize {
        ((self.raw & 0x08) >> 3) as usize
    }

    fn is_horizontally_flipped(&self) -> bool {
        self.raw & 0x20 != 0
    }

    fn is_vertically_flipped(&self) -> bool {
        self.raw & 0x40 != 0
    }

    /// If true, background/window has priority to be drawn on top of objects.
    /// - LCDC priority flag overrides this
    /// - This overrides the object's priority flag
    fn in_foreground(&self) -> bool {
        self.raw & 0x80 != 0
    }
}

/// Convert from screen x coordinate to OAM x coordinate.
fn screen_to_object_x(screen_x: u8) -> u8 {
    screen_x + 8
}

/// Convert from screen y coordinate (aka scanline) to OAM y coordinate.
fn screen_to_object_y(screen_y: u8) -> u8 {
    screen_y + 16
}

/// Total number of objects in OAM.
const NUM_OBJECTS: usize = 40;

const MAX_OBJECTS_PER_SCANLINE: usize = 10;

fn object_height(are_objects_double_size: bool) -> u8 {
    if are_objects_double_size { 16 } else { 8 }
}

/// Collect the first 10 objects whose y-coordinate overlaps with the given scanline.
fn oam_scan(emulator: &Emulator, scanline: u8) -> Vec<Object> {
    let mut objects = Vec::new();
    let oam = &emulator.oam();

    for i in 0..NUM_OBJECTS {
        let start = i * 4;

        // Check if the object is visible on this scanline
        let object_start_y = oam[start];
        let object_end_y =
            object_start_y.wrapping_add(object_height(emulator.is_lcdc_obj_double_size()));
        let scanline_y = screen_to_object_y(scanline);

        if (object_start_y..object_end_y).contains(&scanline_y) {
            objects.push(Object {
                y: object_start_y,
                x: oam[start + 1],
                tile_index: oam[start + 2],
                attributes: oam[start + 3],
            });

            if objects.len() == MAX_OBJECTS_PER_SCANLINE {
                break;
            }
        }
    }

    // In DMG mode, sort by x coordinate (lower x has higher priority). A stable sort is used so
    // that earlier objects in OAM have higher priority when x coordinates are equal.
    if emulator.opri() == 1 {
        objects.sort_by_key(|obj| obj.x);
    }

    objects
}

#[derive(Debug)]
pub enum Color {
    Dmg(DmgColor),
    Cgb(CgbColor),
}

/// A 2-bit color
///   0: White
///   1: Light gray
///   2: Dark gray
///   3: Black
pub type DmgColor = u8;

pub struct CgbColor {
    raw: u16,
}

impl Debug for CgbColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CgbColor")
            .field("red", &self.red())
            .field("green", &self.green())
            .field("blue", &self.blue())
            .finish()
    }
}

impl CgbColor {
    pub fn new(raw: u16) -> Self {
        Self { raw }
    }

    pub fn red(&self) -> u8 {
        (self.raw & 0x1F) as u8
    }

    pub fn green(&self) -> u8 {
        ((self.raw >> 5) & 0x1F) as u8
    }

    pub fn blue(&self) -> u8 {
        ((self.raw >> 10) & 0x1F) as u8
    }
}

const DMG_WHITE_COLOR: Color = Color::Dmg(0);

enum ColorPalette {
    Dmg(u8),
    Cgb(u64),
}

const PALETTE_SIZE: usize = 4;

/// Size of a single CGB color in bytes. CGB colors are stored as 15-bit RGB values.
const CGB_COLOR_SIZE: usize = mem::size_of::<CgbColor>();

/// Size of a single CGB palette in bytes.
const CGB_PALETTE_SIZE: usize = CGB_COLOR_SIZE * PALETTE_SIZE;

/// An index into a palette (0-3).
type ColorIndex = u8;

const TRANSPARENT_COLOR_INDEX: ColorIndex = 0;

/// Returns the color index of the background or window pixel at (x, y) on the screen. Also returns
/// the background tile attributes in CGB mode.
///
/// Returns None for color index if background and window are disabled in DMG mode.
fn background_or_window_color_index(
    emulator: &mut Emulator,
    x: u8,
    y: u8,
) -> (Option<ColorIndex>, Option<BackgroundTileAttributes>) {
    if !emulator.in_cgb_mode() && !emulator.is_lcdc_dmg_bg_window_enabled() {
        return (None, None);
    }

    // Find the tile map coordinates in the window if pixel is in the window
    let window_coordinates = if emulator.is_lcdc_window_enabled() {
        window_tile_map_coordinates(emulator, x, y)
    } else {
        None
    };

    // Otherwise find tile map coordinates in the background
    let is_window = window_coordinates.is_some();
    let mut tile_map_coordinates =
        window_coordinates.unwrap_or_else(|| background_tile_map_coordinates(emulator, x, y));

    // Lookup the actual tile in the tile map
    let tile_index =
        lookup_tile_in_tile_map(emulator, !is_window, tile_map_coordinates.tile_map_index);

    // In CGB mode tile attributes are stored in a second tile map
    let attributes = if emulator.in_cgb_mode() {
        Some(lookup_tile_attributes_in_tile_map(
            emulator,
            !is_window,
            tile_map_coordinates.tile_map_index,
        ))
    } else {
        None
    };

    // Adjust offsets within the file based on flip attributes
    if let Some(attributes) = attributes.as_ref() {
        if attributes.is_horizontally_flipped() {
            tile_map_coordinates.x_offset = 7 - tile_map_coordinates.x_offset;
        }

        if attributes.is_vertically_flipped() {
            tile_map_coordinates.y_offset = 7 - tile_map_coordinates.y_offset;
        }
    }

    // In CGB mode tile attributes specify the VRAM bank
    let vram_bank_num = if let Some(attributes) = attributes.as_ref() {
        attributes.vram_bank_number()
    } else {
        0
    };

    let tile_data_area_addressing_mode = emulator.lcdc_bg_window_tile_data_addressing_mode();

    // Lookup the color index at the offsets within this tile, stored in tile data area
    let color_index = lookup_color_index_in_tile(
        emulator,
        vram_bank_num,
        tile_data_area_addressing_mode,
        tile_index,
        tile_map_coordinates.x_offset,
        tile_map_coordinates.y_offset,
    );

    (Some(color_index), attributes)
}

/// An internal counter used for tracking the tilemap line number for the window. Only incremented
/// when the window itself is rendered on a scanline, instead of incrementing every scanline.
#[derive(Serialize, Deserialize)]
pub struct WindowLineCounter {
    /// The value of the counter
    line: u8,
    /// The last scanline (in screen coordinates) when the counter was updated
    last_updated_scanline: Option<u8>,
}

impl WindowLineCounter {
    pub fn new() -> Self {
        Self {
            line: 0,
            last_updated_scanline: None,
        }
    }

    /// Reset the internal counter at the start of each VBlank.
    pub fn reset(&mut self) {
        self.line = 0;
        self.last_updated_scanline = None;
    }

    /// Get the window coordinate for the given scanline, updating the internal counter the first
    /// time each scanline in screen space is processed.
    fn get_for_scanline(&mut self, screen_scanline: u8) -> u8 {
        if let Some(last_scanline) = self.last_updated_scanline {
            if screen_scanline != last_scanline {
                self.line += 1;
                self.last_updated_scanline = Some(screen_scanline);
            }
        } else {
            self.line = 0;
            self.last_updated_scanline = Some(screen_scanline);
        }

        self.line
    }
}

struct TileMapCoordinates {
    // Index into the 32x32 tile map
    tile_map_index: usize,
    // Offsets within the tile (0-7)
    x_offset: u8,
    y_offset: u8,
}

/// Looks up the tile map index and offsets for the background at the given (x, y) screen
/// coordinates accounting for scroll.
fn background_tile_map_coordinates(emulator: &Emulator, x: u8, y: u8) -> TileMapCoordinates {
    let scx = emulator.scx();
    let scy = emulator.scy();

    // Final pixel index within the 256x256 background
    let background_x = scx.wrapping_add(x);
    let background_y = scy.wrapping_add(y);

    tile_map_coordinates(background_x, background_y)
}

/// Looks up the tile map index and offsets for the window at the given (x, y) screen coordinates
/// accounting for window position.
fn window_tile_map_coordinates(
    emulator: &mut Emulator,
    x: u8,
    y: u8,
) -> Option<TileMapCoordinates> {
    // Window x register is offset by 7 to allow for specifying positions off-screen. Note that we
    // must check for window start values that would be in the range [-7, 0) which are guaranteed to
    // always be before the pixel.
    let wx = emulator.wx();
    let (window_start_x, is_window_start_x_negative) = wx.overflowing_sub(7);
    let window_start_y = emulator.wy();

    // Check if the pixel is within the window both horizontally and vertically
    if (window_start_x > x && !is_window_start_x_negative) || window_start_y > y {
        return None;
    }

    // Final pixel index within the 256x256 window
    let window_x = x.wrapping_sub(window_start_x);
    let window_y = emulator.window_line_counter_mut().get_for_scanline(y);

    Some(tile_map_coordinates(window_x, window_y))
}

/// Number of tiles per row in the tile map.
const TILE_MAP_WIDTH: usize = 32;

/// Convert from coordinates in the 256x256 background or window into the corresponding tile index
/// and offsets within that tile.
fn tile_map_coordinates(x: u8, y: u8) -> TileMapCoordinates {
    let tile_map_x = x / 8;
    let tile_map_y = y / 8;

    let x_offset = x % 8;
    let y_offset = y % 8;

    let tile_map_index = tile_map_y as usize * TILE_MAP_WIDTH + tile_map_x as usize;

    TileMapCoordinates {
        tile_map_index,
        x_offset,
        y_offset,
    }
}

const TILE_MAP_1_ADDRESS: usize = 0x9800;
const TILE_MAP_2_ADDRESS: usize = 0x9C00;

/// Lookup a tile map index to the corresponding byte in the tile data area.
///
/// This byte may be interpreted as a tile index or tile attributes depending on context.
///
/// Must specify whether looking up background or window tile map, as they can be different.
fn lookup_byte_in_tile_map(
    emulator: &Emulator,
    vram_bank_num: usize,
    is_background: bool,
    tile_map_index: usize,
) -> u8 {
    let tile_map_number = if is_background {
        emulator.lcdc_bg_tile_map_number()
    } else {
        emulator.lcdc_window_tile_map_number()
    };

    let tile_map_base = if tile_map_number == 0 {
        TILE_MAP_1_ADDRESS
    } else {
        TILE_MAP_2_ADDRESS
    };

    // Tile index is always in VRAM bank 0
    let vram_addr =
        Emulator::map_vram_address_in_bank((tile_map_base + tile_map_index) as u16, vram_bank_num);

    emulator.vram()[vram_addr]
}

/// Lookup a tile map index to get the corresponding tile index in the tile data area.
fn lookup_tile_in_tile_map(emulator: &Emulator, is_background: bool, tile_map_index: usize) -> u8 {
    // Tile index are always in VRAM bank 0
    lookup_byte_in_tile_map(emulator, 0, is_background, tile_map_index)
}

/// Lookup a tile map index to get the corresponding tile attributes (in CGB mode).
fn lookup_tile_attributes_in_tile_map(
    emulator: &Emulator,
    is_background: bool,
    tile_map_index: usize,
) -> BackgroundTileAttributes {
    // Tile attributes are always in VRAM bank 1
    let raw = lookup_byte_in_tile_map(emulator, 1, is_background, tile_map_index);
    BackgroundTileAttributes { raw }
}

const TILE_DATA_1_BASE_ADDRESS: usize = 0x8000;
const TILE_DATA_2_BASE_ADDRESS: usize = 0x9000;

/// Objects always use the first tile data area.
const OBJECT_TILE_DATA_ADDRESSING_MODE: u8 = 1;

const TILE_DATA_SIZE: usize = 16;

/// Lookup the color index at the given pixel offsets within the specified tile.
///
/// Use the tile data area provided (0 or 1).
fn lookup_color_index_in_tile(
    emulator: &Emulator,
    vram_bank_num: usize,
    tile_data_area_addressing_mode: u8,
    tile_index: u8,
    x_offset: u8,
    y_offset: u8,
) -> ColorIndex {
    // Calculate the start of the tile data
    let tile_data_start = if tile_data_area_addressing_mode == 1 {
        TILE_DATA_1_BASE_ADDRESS + (tile_index as usize * TILE_DATA_SIZE)
    } else {
        // In 0x8800 addressing mode the tile index is interpreted as signed
        let tile_index_offset = tile_index as i8 as isize * TILE_DATA_SIZE as isize;
        TILE_DATA_2_BASE_ADDRESS.wrapping_add_signed(tile_index_offset)
    };

    // Each line in the tile is represented by 2 bytes
    let line_data_start = tile_data_start + (y_offset as usize * 2);

    // Calculate the physical VRAM address
    let vram_addr = Emulator::map_vram_address_in_bank(line_data_start as u16, vram_bank_num);

    let mask = 1 << (7 - x_offset);

    let low_bit = emulator.vram()[vram_addr] & mask != 0;
    let high_bit = emulator.vram()[vram_addr + 1] & mask != 0;

    ((high_bit as u8) << 1) | (low_bit as u8)
}

/// Lookup the 2-bit color for the given color index in a palette.
fn lookup_color_in_palette(palette: &ColorPalette, color_index: ColorIndex) -> Color {
    match palette {
        ColorPalette::Dmg(palette) => {
            // DMG color is a 2-bit value
            Color::Dmg(palette >> (color_index * 2) & 0x03)
        }
        ColorPalette::Cgb(palette) => Color::Cgb(CgbColor::new(
            (palette >> (color_index * 16)) as u16 & 0x7FFF,
        )),
    }
}

fn lookup_cgb_palette(cgb_palletes: &CgbPaletteData, palette_number: usize) -> ColorPalette {
    let start = palette_number * CGB_PALETTE_SIZE;
    let palette_slice = &cgb_palletes[start..(start + CGB_PALETTE_SIZE)];

    ColorPalette::Cgb(u64::from_le_bytes(palette_slice.try_into().unwrap()))
}

fn background_color_palette(
    emulator: &Emulator,
    attributes: Option<&BackgroundTileAttributes>,
) -> ColorPalette {
    // TODO: Handle CGB's DMG compatibility mode
    if emulator.in_cgb_mode() {
        return lookup_cgb_palette(
            emulator.cgb_background_palettes(),
            attributes.unwrap().color_palette(),
        );
    }

    ColorPalette::Dmg(emulator.bgp())
}

fn object_color_palette(emulator: &Emulator, object: &Object) -> ColorPalette {
    // TODO: Handle CGB's DMG compatibility mode
    if emulator.in_cgb_mode() {
        return lookup_cgb_palette(emulator.cgb_object_palettes(), object.cgb_pallette_number());
    }

    if object.dmg_palette_number() == 0 {
        ColorPalette::Dmg(emulator.obp0())
    } else {
        ColorPalette::Dmg(emulator.obp1())
    }
}

pub fn draw_scanline(emulator: &mut Emulator, scanline: u8) {
    // Find the first 10 objects that intersect with this scanline
    let objects = oam_scan(emulator, scanline);

    let are_objects_double_size = emulator.is_lcdc_obj_double_size();
    let object_height = object_height(are_objects_double_size);

    for x in 0..(SCREEN_WIDTH as u8) {
        let (background_color_index, background_attributes) =
            background_or_window_color_index(emulator, x, scanline);
        let background_palette = background_color_palette(emulator, background_attributes.as_ref());

        let mut final_color_index_and_palette = (background_color_index, background_palette);

        if emulator.is_lcdc_obj_enabled() {
            for object in &objects {
                let current_object_x = screen_to_object_x(x);
                let current_object_y = screen_to_object_y(scanline);

                // Check if object intersects the current x coordinate
                if current_object_x < object.x || current_object_x >= object.x + 8 {
                    continue;
                }

                // Find the offsets within the object's tile
                let x_offset = if object.is_horizontally_flipped() {
                    7 - (current_object_x - object.x)
                } else {
                    current_object_x - object.x
                };

                let mut y_offset = if object.is_vertically_flipped() {
                    (object_height - 1) - (current_object_y - object.y)
                } else {
                    current_object_y - object.y
                };

                let tile_index = if are_objects_double_size {
                    // In double tile mode the lower bit of the tile index is ignored and must be
                    // set to 1 to access the second tile if pixel appears in the second tile.
                    if y_offset >= 8 {
                        y_offset -= 8;
                        object.tile_index | 0x01
                    } else {
                        object.tile_index & 0xFE
                    }
                } else {
                    object.tile_index
                };

                // In CGB mode object attributes specify the VRAM bank
                let vram_bank_num = if emulator.in_cgb_mode() {
                    object.vram_bank_number()
                } else {
                    0
                };

                // Find the color index for the pixel at those offsets in the tile
                let object_color_index = lookup_color_index_in_tile(
                    emulator,
                    vram_bank_num,
                    OBJECT_TILE_DATA_ADDRESSING_MODE,
                    tile_index,
                    x_offset,
                    y_offset,
                );

                // If the object's color index is transparent then search for the next object
                if object_color_index == TRANSPARENT_COLOR_INDEX {
                    continue;
                }

                // Background attributes are only present in CGB mode
                let is_object_on_top = if let Some(background_attributes) =
                    background_attributes.as_ref()
                {
                    // Object is drawn on top of transparent background
                    matches!(background_color_index, Some(TRANSPARENT_COLOR_INDEX)) ||
                    // Object is drawn on top if lcdc priority flag forces bg/window behind objects
                    !emulator.is_lcdc_cgb_bg_window_priority() ||
                    // Object in background and bg/window in foreground flags are considered, with
                    // bg/window flag overriding when necessary.
                    (!object.in_background() && !background_attributes.in_foreground())
                } else {
                    // Object is not transparent so it will always be rendered unless flagged to be
                    // in the background and the background is non-transparent.
                    !object.in_background()
                        || matches!(background_color_index, None | Some(TRANSPARENT_COLOR_INDEX))
                };

                if is_object_on_top {
                    let object_palette = object_color_palette(emulator, object);
                    final_color_index_and_palette = (Some(object_color_index), object_palette);
                }

                // Always stop after we find a non-transparent object pixel, even if the background
                // should be draw on top of this object pixel.
                break;
            }
        }

        // Finally lookup color from the palette
        let (color_index, palette) = final_color_index_and_palette;

        let color = if let Some(color_index) = color_index {
            lookup_color_in_palette(&palette, color_index)
        } else {
            // No color so pixel defaults to white. This can only occur in DMG mode.
            DMG_WHITE_COLOR
        };

        emulator.write_color(x, scanline, color);
    }
}
