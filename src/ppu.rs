use crate::emulator::{Emulator, SCREEN_WIDTH};

/// A sprite in OAM.
struct Object {
    y: u8,
    x: u8,
    tile_index: u8,
    attributes: u8,
}

impl Object {
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

    /// If false, object is drawn on top of background (and window) if not transparent.
    /// If true, object is behind background (and window) unless background color is transparent.
    fn in_background(&self) -> bool {
        self.attributes & 0x80 != 0
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

/// Collect the first 10 objects whose y-coordinate overlaps with the given scanline.
fn oam_scan(emulator: &Emulator, scanline: u8) -> Vec<Object> {
    let mut objects = Vec::new();
    let oam = &emulator.oam();

    for i in 0..NUM_OBJECTS {
        let start = i * 4;

        // Check if the object is visible on this scanline
        // TODO: Handle 16 pixel sprites
        let object_start_y = oam[start];
        let object_end_y = object_start_y + 8;
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

    objects
}

/// A 2-bit color
///   0: White
///   1: Light gray
///   2: Dark gray
///   3: Black
pub type Color = u8;

const WHITE_COLOR: Color = 0;

/// An index into a palette (0-3).
type ColorIndex = u8;

const TRANSPARENT_COLOR_INDEX: ColorIndex = 0;

/// Returns the color index of the background or window pixel at (x, y) on the screen.
///
/// Returns None if background and window are disabled.
fn background_or_window_color_index(emulator: &Emulator, x: u8, y: u8) -> Option<ColorIndex> {
    if !emulator.io_registers().lcdc_bg_window_enable() {
        return None;
    }

    // Find the tile map coordinates in the window if pixel is in the window
    let window_coordinates = if emulator.io_registers().lcdc_window_enable() {
        window_tile_map_coordinates(emulator, x, y)
    } else {
        None
    };

    // Otherwise find tile map coordinates in the background
    let is_window = window_coordinates.is_some();
    let tile_map_coordinates =
        window_coordinates.unwrap_or_else(|| background_tile_map_coordinates(emulator, x, y));

    // Lookup the actual tile in the tile map
    let tile_index =
        lookup_tile_in_tile_map(emulator, !is_window, tile_map_coordinates.tile_map_index);

    let tile_data_area_number =
        (emulator.io_registers().lcdc_bg_window_tile_data_area() == 0) as u8;

    // Lookup the color index at the offsets within this tile, stored in tile data area
    let color_index = lookup_color_index_in_tile(
        emulator,
        tile_data_area_number,
        tile_index,
        tile_map_coordinates.x_offset,
        tile_map_coordinates.y_offset,
    );

    Some(color_index)
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
    let scx = emulator.io_registers().scx();
    let scy = emulator.io_registers().scy();

    // Final pixel index within the 256x256 background
    let background_x = scx.wrapping_add(x);
    let background_y = scy.wrapping_add(y);

    tile_map_coordinates(background_x, background_y)
}

/// Looks up the tile map index and offsets for the window at the given (x, y) screen coordinates
/// accounting for window position.
fn window_tile_map_coordinates(emulator: &Emulator, x: u8, y: u8) -> Option<TileMapCoordinates> {
    // Window x register is offset by 7 to allow for specifying positions off-screen
    let window_start_x = emulator.io_registers().wx() - 7;
    let window_start_y = emulator.io_registers().wy();

    // Check if the pixel is within the window both horizontally and vertically
    if window_start_x > x || window_start_y > y {
        return None;
    }

    // Final pixel index within the 256x256 window
    let window_x = x.wrapping_sub(window_start_x);
    let window_y = y.wrapping_sub(window_start_y);

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

/// Lookup a tile map index to get the corresponding tile index in the tile data area.
///
/// Must specify whether looking up background or window tile map, as they can be different.
fn lookup_tile_in_tile_map(emulator: &Emulator, is_background: bool, tile_map_index: usize) -> u8 {
    let tile_map_number = if is_background {
        emulator.io_registers().lcdc_bg_tile_map_number()
    } else {
        emulator.io_registers().lcdc_window_tile_map_number()
    };

    let tile_map_base = if tile_map_number == 0 {
        TILE_MAP_1_ADDRESS
    } else {
        TILE_MAP_2_ADDRESS
    };

    emulator.read_address((tile_map_base + tile_map_index) as u16)
}

const TILE_DATA_1_ADDRESS: usize = 0x8000;
const TILE_DATA_2_ADDRESS: usize = 0x8800;

/// Objects always use the first tile data area.
const OBJECT_TILE_DATA_NUMBER: u8 = 0;

const TILE_DATA_SIZE: usize = 16;

/// Lookup the color index at the given pixel offsets within the specified tile.
///
/// Use the tile data area provided (0 or 1).
fn lookup_color_index_in_tile(
    emulator: &Emulator,
    tile_data_area_number: u8,
    tile_index: u8,
    x_offset: u8,
    y_offset: u8,
) -> ColorIndex {
    let tile_data_base = if tile_data_area_number == 0 {
        TILE_DATA_1_ADDRESS
    } else {
        TILE_DATA_2_ADDRESS
    };

    // Calculate the start of the tile data
    let tile_data_start = tile_data_base + (tile_index as usize * TILE_DATA_SIZE);

    // Each line in the tile is represented by 2 bytes
    let line_data_start = tile_data_start + (y_offset as usize * 2);

    let mask = 1 << (7 - x_offset);

    let low_bit = emulator.read_address(line_data_start as u16) & mask != 0;
    let high_bit = emulator.read_address((line_data_start + 1) as u16) & mask != 0;

    ((high_bit as u8) << 1) | (low_bit as u8)
}

/// Lookup the 2-bit color for the given color index in a palette.
fn lookup_color_in_palette(palette: u8, color_index: ColorIndex) -> Color {
    palette >> (color_index * 2) & 0x03
}

fn object_color_palette(emulator: &Emulator, object: &Object) -> u8 {
    if object.dmg_palette_number() == 0 {
        emulator.io_registers().obp0()
    } else {
        emulator.io_registers().obp1()
    }
}

pub fn draw_scanline(emulator: &mut Emulator, scanline: u8) {
    // Find the first 10 objects that intersect with this scanline
    let objects = oam_scan(emulator, scanline);

    for x in 0..(SCREEN_WIDTH as u8) {
        let background_color_index = background_or_window_color_index(emulator, x, scanline);

        let mut final_color_index_and_palette =
            (background_color_index, emulator.io_registers().bgp());

        if emulator.io_registers().lcdc_obj_enable() {
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

                let y_offset = if object.is_vertically_flipped() {
                    7 - (current_object_y - object.y)
                } else {
                    current_object_y - object.y
                };

                // Find the color index for the pixel at those offsets in the tile
                let object_color_index = lookup_color_index_in_tile(
                    emulator,
                    OBJECT_TILE_DATA_NUMBER,
                    object.tile_index,
                    x_offset,
                    y_offset,
                );

                // If the object's color index is transparent then search for the next object
                if object_color_index == TRANSPARENT_COLOR_INDEX {
                    continue;
                }

                // Object is not transparent so it will always be rendered unless flagged to be in
                // the background and the background is non-transparent.
                let is_object_on_top = !object.in_background()
                    || matches!(background_color_index, None | Some(TRANSPARENT_COLOR_INDEX));

                if is_object_on_top {
                    let object_palette = object_color_palette(emulator, object);
                    final_color_index_and_palette = (Some(object_color_index), object_palette);
                }
            }
        }

        // Finally lookup color from the palette
        let (color_index, palette) = final_color_index_and_palette;

        let color = if let Some(color_index) = color_index {
            lookup_color_in_palette(palette, color_index)
        } else {
            // No color so pixel defaults to white
            WHITE_COLOR
        };

        emulator.write_color(x, scanline, color);
    }
}
