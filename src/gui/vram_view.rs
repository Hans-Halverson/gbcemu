use eframe::egui::{self, CornerRadius, Pos2, Rect, Vec2, ViewportId};

use crate::{
    emulator::{SCREEN_HEIGHT, SCREEN_WIDTH, to_output_color},
    gui::shell::EmulatorShellApp,
    ppu::{
        TILE_MAP_SIZE, TILE_MAP_TOTAL_TILES, TILE_SIZE, background_color_palette,
        lookup_all_pixels_in_tile, lookup_byte_in_tile_map, lookup_color_in_palette,
    },
};

/// Number of screen pixels per emulated pixel in the VRAM view
const SCALE_FACTOR: f32 = 2.0;
const WINDOW_PADDING: f32 = 10.0;
const OPTIONS_WIDTH: f32 = 300.0;

/// Position of the top-left corner of the VRAM view in the window
const PIXELS_AREA_TOP_LEFT: Vec2 = Vec2::new(OPTIONS_WIDTH + WINDOW_PADDING, WINDOW_PADDING);

const TOP_LEFT_PIXEL_PAINTER_COORDS: Pos2 = EmulatorShellApp::pixel_to_painter_coords(0, 0);
const BOTTOM_RIGHT_PIXEL_PAINTER_COORDS: Pos2 = EmulatorShellApp::pixel_to_painter_coords(256, 256);

const WINDOW_HEIGHT: f32 = (256.0 * SCALE_FACTOR) + (2.0 * WINDOW_PADDING);
const WINDOW_WIDTH: f32 = OPTIONS_WIDTH + WINDOW_HEIGHT;
const WINDOW_SIZE: Vec2 = Vec2::new(WINDOW_WIDTH, WINDOW_HEIGHT);

#[derive(PartialEq)]
enum Layer {
    Background,
    Window,
}

#[derive(PartialEq)]
enum TileMap {
    /// 0x9800 - 0x9BFF
    One,
    /// 0x9C00 - 0x9FFF
    Two,
}

#[derive(PartialEq)]
enum TileDataAddressingMode {
    /// 0x8000 + u8
    Signed,
    /// 0x9000 + i8
    Unsigned,
}

pub struct VramViewOptions {
    layer: Layer,
    tile_map: Option<TileMap>,
    tile_data_addressing_mode: Option<TileDataAddressingMode>,
}

impl VramViewOptions {
    pub fn new() -> Self {
        VramViewOptions {
            layer: Layer::Background,
            tile_map: None,
            tile_data_addressing_mode: None,
        }
    }
}

impl EmulatorShellApp {
    pub fn vram_viewport_id(&self) -> ViewportId {
        ViewportId::from_hash_of("vram_viewport_id")
    }

    pub(super) fn draw_vram_viewport(&mut self, ui: &mut egui::Ui) {
        ui.ctx().show_viewport_immediate(
            self.vram_viewport_id(),
            egui::ViewportBuilder::default()
                .with_inner_size(WINDOW_SIZE)
                .with_resizable(false)
                .with_active(true)
                .with_title("VRAM View"),
            |ctx, _| egui::CentralPanel::default().show(ctx, |ui| self.draw_vram_view(ui)),
        );
    }

    pub fn draw_vram_view(&mut self, ui: &mut egui::Ui) {
        egui::Frame::NONE.inner_margin(10.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                self.draw_vram_options(ui);
                self.draw_vram_pixels_area(ui);
            })
        });
    }

    fn draw_vram_pixels_area(&self, ui: &mut egui::Ui) {
        const VRAM_BANK: usize = 0;

        let painter = ui.painter();

        for i in 0..TILE_MAP_TOTAL_TILES {
            let tile_index = lookup_byte_in_tile_map(
                self.emulator(),
                VRAM_BANK,
                self.tile_map_number_from_option(),
                i,
            );

            let tile_pixels = lookup_all_pixels_in_tile(
                self.emulator(),
                VRAM_BANK,
                self.tile_data_addressing_mode_from_option(),
                tile_index,
            );

            // Top left corner of the tile
            let tile_start_x = (i % TILE_MAP_SIZE) * TILE_SIZE;
            let tile_start_y = (i / TILE_MAP_SIZE) * TILE_SIZE;

            for x in 0..TILE_SIZE {
                for y in 0..TILE_SIZE {
                    let color_index = tile_pixels[y][x];

                    let color = lookup_color_in_palette(
                        &background_color_palette(&self.emulator(), None),
                        color_index,
                    );

                    let pixel_x = tile_start_x + x;
                    let pixel_y = tile_start_y + y;
                    let pixel_rect = Rect::from_two_pos(
                        Self::pixel_to_painter_coords(pixel_x, pixel_y),
                        Self::pixel_to_painter_coords(pixel_x + 1, pixel_y + 1),
                    );

                    painter.rect_filled(pixel_rect, CornerRadius::ZERO, to_output_color(color));
                }
            }
        }

        // Draw border around the entire VRAM view
        self.draw_debugger_vram_border(painter);

        // Draw border around the currently selected layer
        match self.vram_view_options().layer {
            Layer::Background => self.draw_background_border(painter),
            Layer::Window => self.draw_window_border(painter),
        }
    }

    fn bg_window_border_stroke() -> egui::Stroke {
        egui::Stroke::new(2.0, egui::Color32::RED)
    }

    /// Draw a border around the currently visible window area in the VRAM view.
    fn draw_window_border(&self, painter: &egui::Painter) {
        let wx = self.emulator().wx().saturating_sub(7);
        let wy = self.emulator().wy();

        let start_x = Self::x_pixel_to_painter_coords(wx);
        let start_y = Self::y_pixel_to_painter_coords(wy);

        let end_x = Self::x_pixel_to_painter_coords_usize(SCREEN_WIDTH);
        let end_y = Self::y_pixel_to_painter_coords_usize(SCREEN_HEIGHT);

        painter.rect_stroke(
            Rect::from_x_y_ranges(start_x..=end_x, start_y..=end_y),
            0.0,
            Self::bg_window_border_stroke(),
            egui::StrokeKind::Outside,
        );
    }

    /// Draw a border around the currently visible background area in the VRAM view.
    ///
    /// The border is drawn in red and wraps around to the other side of the screen.
    fn draw_background_border(&self, painter: &egui::Painter) {
        let stroke = Self::bg_window_border_stroke();

        let x_start = self.emulator().scx();
        let y_start = self.emulator().scy();

        let left_border_painter_x = Self::x_pixel_to_painter_coords(x_start) - 1.0;
        let top_border_painter_y = Self::y_pixel_to_painter_coords(y_start) - 1.0;
        let (right_border_painter_x, x_overflowed) = Self::right_border_painter_x(x_start);
        let (bottom_border_painter_y, y_overflowed) = Self::bottom_border_painter_y(y_start);

        let left_edge_painter_x = TOP_LEFT_PIXEL_PAINTER_COORDS.x;
        let top_edge_painter_y = TOP_LEFT_PIXEL_PAINTER_COORDS.y;
        let right_edge_painter_x = BOTTOM_RIGHT_PIXEL_PAINTER_COORDS.x;
        let bottom_edge_painter_y = BOTTOM_RIGHT_PIXEL_PAINTER_COORDS.y;

        // 1px longer on both sides to fill in corners.
        if x_overflowed {
            let start_to_right_edge = (left_border_painter_x - 1.0)..=right_edge_painter_x;
            let left_edge_to_end = left_edge_painter_x..=(right_border_painter_x + 1.0);

            painter.hline(&start_to_right_edge, top_border_painter_y, stroke);
            painter.hline(&left_edge_to_end, top_border_painter_y, stroke);

            painter.hline(&start_to_right_edge, bottom_border_painter_y, stroke);
            painter.hline(&left_edge_to_end, bottom_border_painter_y, stroke);
        } else {
            let range = (left_border_painter_x - 1.0)..=(right_border_painter_x + 1.0);

            painter.hline(&range, top_border_painter_y, stroke);
            painter.hline(&range, bottom_border_painter_y, stroke);
        }

        if y_overflowed {
            let start_to_bottom_edge = top_border_painter_y..=bottom_edge_painter_y;
            let top_edge_to_end = top_edge_painter_y..=bottom_border_painter_y;

            painter.vline(left_border_painter_x, &start_to_bottom_edge, stroke);
            painter.vline(left_border_painter_x, &top_edge_to_end, stroke);

            painter.vline(right_border_painter_x, &start_to_bottom_edge, stroke);
            painter.vline(right_border_painter_x, &top_edge_to_end, stroke);
        } else {
            let range = top_border_painter_y..=bottom_border_painter_y;

            painter.vline(left_border_painter_x, &range, stroke);
            painter.vline(right_border_painter_x, &range, stroke);
        }
    }

    fn draw_debugger_vram_border(&self, painter: &egui::Painter) {
        painter.rect_stroke(
            Rect::from_two_pos(
                TOP_LEFT_PIXEL_PAINTER_COORDS,
                BOTTOM_RIGHT_PIXEL_PAINTER_COORDS,
            ),
            CornerRadius::ZERO,
            egui::Stroke::new(2.0, egui::Color32::BLACK),
            egui::StrokeKind::Outside,
        );
    }

    const fn pixel_to_painter_coords(x: usize, y: usize) -> Pos2 {
        Pos2::new(
            Self::x_pixel_to_painter_coords_usize(x),
            Self::y_pixel_to_painter_coords_usize(y),
        )
    }

    const fn x_pixel_to_painter_coords_usize(x: usize) -> f32 {
        (x as f32) * SCALE_FACTOR + PIXELS_AREA_TOP_LEFT.x
    }

    const fn y_pixel_to_painter_coords_usize(y: usize) -> f32 {
        (y as f32) * SCALE_FACTOR + PIXELS_AREA_TOP_LEFT.y
    }

    const fn x_pixel_to_painter_coords(x: u8) -> f32 {
        (x as f32) * SCALE_FACTOR + PIXELS_AREA_TOP_LEFT.x
    }

    const fn y_pixel_to_painter_coords(y: u8) -> f32 {
        (y as f32) * SCALE_FACTOR + PIXELS_AREA_TOP_LEFT.y
    }

    fn right_border_painter_x(x_start: u8) -> (f32, bool) {
        let mut x_end = (x_start as usize) + SCREEN_WIDTH;
        let x_overflowed = x_end > 256;
        if x_overflowed {
            x_end -= 256;
        }

        (
            Self::x_pixel_to_painter_coords_usize(x_end) + 1.0,
            x_overflowed,
        )
    }

    fn bottom_border_painter_y(y_start: u8) -> (f32, bool) {
        let mut y_end = (y_start as usize) + SCREEN_HEIGHT;
        let y_overflowed = y_end > 256;
        if y_overflowed {
            y_end -= 256;
        }

        (
            Self::y_pixel_to_painter_coords_usize(y_end) + 1.0,
            y_overflowed,
        )
    }

    fn draw_vram_options(&mut self, ui: &mut egui::Ui) {
        const VERTICAL_GAP: f32 = 20.0;

        ui.vertical(|ui| {
            self.draw_layer_option(ui);

            ui.add_space(VERTICAL_GAP);
            self.draw_tile_map_option(ui);

            ui.add_space(VERTICAL_GAP);
            self.draw_tile_data_addressing_mode_option(ui);
        });
    }

    fn draw_layer_option(&mut self, ui: &mut egui::Ui) {
        ui.label("Layer:");

        let layer = &mut self.vram_view_options_mut().layer;
        ui.radio_value(layer, Layer::Background, "Background");
        ui.radio_value(layer, Layer::Window, "Window");
    }

    fn current_layer_tile_map_number(&self) -> u8 {
        match self.vram_view_options().layer {
            Layer::Background => self.emulator().lcdc_bg_tile_map_number(),
            Layer::Window => self.emulator().lcdc_window_tile_map_number(),
        }
    }

    fn tile_map_number_from_option(&self) -> u8 {
        match self.vram_view_options().tile_map {
            None => self.current_layer_tile_map_number(),
            Some(TileMap::One) => 0,
            Some(TileMap::Two) => 1,
        }
    }

    fn draw_tile_map_option(&mut self, ui: &mut egui::Ui) {
        let tile_map_one_label = "0x9800 - 0x9BFF";
        let tile_map_two_label = "0x9C00 - 0x9FFF";

        let current_tile_map_label = match self.current_layer_tile_map_number() {
            0 => tile_map_one_label,
            1 => tile_map_two_label,
            _ => unreachable!(),
        };

        ui.label("Tile Map:");

        let tile_map = &mut self.vram_view_options_mut().tile_map;
        ui.radio_value(
            tile_map,
            None,
            format!("Current ({})", current_tile_map_label),
        );
        ui.radio_value(tile_map, Some(TileMap::One), tile_map_one_label);
        ui.radio_value(tile_map, Some(TileMap::Two), tile_map_two_label);
    }

    fn tile_data_addressing_mode_from_option(&self) -> u8 {
        match self.vram_view_options().tile_data_addressing_mode {
            None => self.emulator().lcdc_bg_window_tile_data_addressing_mode(),
            Some(TileDataAddressingMode::Unsigned) => 0,
            Some(TileDataAddressingMode::Signed) => 1,
        }
    }

    fn draw_tile_data_addressing_mode_option(&mut self, ui: &mut egui::Ui) {
        let unsigned_mode_label = "0x8000 + unsigned";
        let signed_mode_label = "0x9000 + signed";

        let current_mode_label = match self.emulator().lcdc_bg_window_tile_data_addressing_mode() {
            0 => unsigned_mode_label,
            1 => signed_mode_label,
            _ => unreachable!(),
        };

        ui.label("Tile Data Addressing Mode:");

        let addressing_mode = &mut self.vram_view_options_mut().tile_data_addressing_mode;
        ui.radio_value(
            addressing_mode,
            None,
            format!("Current ({})", current_mode_label),
        );
        ui.radio_value(
            addressing_mode,
            Some(TileDataAddressingMode::Unsigned),
            unsigned_mode_label,
        );
        ui.radio_value(
            addressing_mode,
            Some(TileDataAddressingMode::Signed),
            signed_mode_label,
        );
    }
}
