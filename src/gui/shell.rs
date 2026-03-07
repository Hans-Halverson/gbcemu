use std::{sync::mpsc::Sender, time::Duration};

use eframe::{
    egui::{self, Align2, Color32, FontId, Key, Pos2, Vec2, ViewportCommand, style::ScrollStyle},
    epaint::CornerRadius,
};
use muda::Menu;

use crate::{
    emulator::{Button, Command, Emulator, EmulatorRef, SCREEN_HEIGHT, SCREEN_WIDTH},
    gui::{
        debugger_view::{DebuggerViewport, WINDOW_INNER_SIZE as DEBUGGER_WINDOW_INNER_SIZE},
        menu::create_app_menu,
        utils::rect_for_coordinate,
        vram_view::VramViewport,
    },
    ppu::Color,
};

/// The color palettes available for DMG (non-CGB) games.
#[derive(Clone, Copy, PartialEq)]
pub enum ScreenColorPalette {
    Grayscale,
    Green,
}

/// The default grayscale color palette.
pub const SCREEN_COLOR_PALETTE_GRAYSCALE: [Color32; 4] = [
    Color32::from_rgb(0xFF, 0xFF, 0xFF),
    Color32::from_rgb(0xAA, 0xAA, 0xAA),
    Color32::from_rgb(0x55, 0x55, 0x55),
    Color32::from_rgb(0x00, 0x00, 0x00),
];

/// A green color palette for the original GameBoy screen.
pub const SCREEN_COLOR_PALETTE_GREEN: [Color32; 4] = [
    Color32::from_rgb(0x9B, 0xBC, 0x0F),
    Color32::from_rgb(0x8B, 0xAC, 0x0F),
    Color32::from_rgb(0x30, 0x62, 0x30),
    Color32::from_rgb(0x0F, 0x38, 0x0F),
];

/// Number of screen pixels per emulated pixel by default
const DEFAULT_SCALE_FACTOR: f32 = 4.0;

pub fn start_emulator_shell_app(emulator: EmulatorRef, commands_tx: Sender<Command>) {
    eframe::run_native(
        "GBC Emulator",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([
                    (SCREEN_WIDTH as f32) * DEFAULT_SCALE_FACTOR,
                    (SCREEN_HEIGHT as f32) * DEFAULT_SCALE_FACTOR,
                ])
                .with_min_inner_size([SCREEN_WIDTH as f32, SCREEN_HEIGHT as f32])
                .with_active(true)
                .with_transparent(true)
                .with_title_shown(true),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(EmulatorShellApp::new(emulator, commands_tx)))),
    )
    .unwrap()
}

/// Target frames per second for the GUI to refresh
const GUI_FPS: f64 = 60.0;

pub struct EmulatorShellApp {
    /// Reference to the emulator
    emulator: EmulatorRef,

    /// Channel to send commands to the emulator
    commands_tx: Sender<Command>,

    /// Set of buttons that were pressed last frame
    pressed_buttons: u8,

    /// Whether we are currently in turbo mode, speeding up the emulation
    in_turbo_mode: bool,

    /// Whether the FPS counter should be shown onscreen
    show_fps: bool,

    /// The active color palette for DMG games, translating from 2-bit color indices to RGB values.
    screen_palette: ScreenColorPalette,

    /// The VRAM viewport state
    vram_view: VramViewport,

    /// The debugger viewport state
    debugger_view: DebuggerViewport,

    /// The app menu. Must be kept alive for the menu to function.
    menu: Menu,

    /// Whether the app has been initialized
    is_initialized: bool,
}

impl EmulatorShellApp {
    fn new(emulator: EmulatorRef, commands_tx: Sender<Command>) -> Self {
        let menu = create_app_menu();

        Self {
            emulator,
            commands_tx,
            pressed_buttons: 0,
            in_turbo_mode: false,
            show_fps: false,
            screen_palette: ScreenColorPalette::Grayscale,
            vram_view: VramViewport::new(),
            debugger_view: DebuggerViewport::new(),
            menu,
            is_initialized: false,
        }
    }

    pub fn emulator(&self) -> &Emulator {
        &self.emulator
    }

    pub fn set_color_palette(&mut self, screen_palette: ScreenColorPalette) {
        self.screen_palette = screen_palette;
        self.update_color_palette_menu(screen_palette);
    }

    pub fn menu(&self) -> &Menu {
        &self.menu
    }

    pub fn send_command(&self, command: Command) {
        self.commands_tx.send(command).unwrap();
    }

    fn init(&mut self, ctx: &egui::Context) {
        self.is_initialized = true;

        self.init_styles(ctx);
    }

    fn init_styles(&self, ctx: &egui::Context) {
        // Floating scrollbars allows for scroll areas to have a constant inner width
        ctx.style_mut(|s| s.spacing.scroll = ScrollStyle::floating());
    }

    fn handle_pressed_buttons(&mut self, ctx: &egui::Context) {
        let mut buttons = 0;
        if ctx.input(|i| i.key_down(Key::A)) {
            buttons |= Button::Select as u8;
        }
        if ctx.input(|i| i.key_down(Key::S)) {
            buttons |= Button::Start as u8;
        }
        if ctx.input(|i| i.key_down(Key::Z)) {
            buttons |= Button::B as u8;
        }
        if ctx.input(|i| i.key_down(Key::X)) {
            buttons |= Button::A as u8;
        }
        if ctx.input(|i| i.key_down(Key::ArrowUp)) {
            buttons |= Button::Up as u8;
        }
        if ctx.input(|i| i.key_down(Key::ArrowDown)) {
            buttons |= Button::Down as u8;
        }
        if ctx.input(|i| i.key_down(Key::ArrowLeft)) {
            buttons |= Button::Left as u8;
        }
        if ctx.input(|i| i.key_down(Key::ArrowRight)) {
            buttons |= Button::Right as u8;
        }

        if buttons != self.pressed_buttons {
            self.pressed_buttons = buttons;
            self.send_command(Command::UpdatePressedButtons(buttons));
        }
    }

    fn handle_turbo_mode(&mut self, ctx: &egui::Context) {
        let in_turbo_mode = ctx.input(|i| i.key_down(Key::Space));
        if in_turbo_mode != self.in_turbo_mode {
            self.in_turbo_mode = in_turbo_mode;
            self.send_command(Command::SetTurboMode(in_turbo_mode));
        }
    }

    fn draw(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_emulator_viewport(ui);

            if self.vram_view().is_shown() {
                self.draw_vram_viewport(ui);
            }

            if self.debugger_view().is_shown() {
                self.draw_debugger_viewport(ui);
            }
        });
    }

    fn draw_emulator_viewport(&self, ui: &mut egui::Ui) {
        self.draw_screen(ui);

        if self.show_fps {
            self.draw_frame_rate_counter(ui);
        }
    }

    pub fn color_to_color32(&self, color: Color) -> Color32 {
        let palette = match self.screen_palette {
            ScreenColorPalette::Grayscale => SCREEN_COLOR_PALETTE_GRAYSCALE,
            ScreenColorPalette::Green => SCREEN_COLOR_PALETTE_GREEN,
        };
        match color {
            Color::Dmg(idx) => palette[idx as usize],
            Color::Cgb(cgb) => cgb.to_color32(),
        }
    }

    fn draw_screen(&self, ui: &mut egui::Ui) {
        let scale_factor = self.calculate_scale_factor(ui.ctx());
        let painter = ui.painter();

        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let color32 = self.color_to_color32(self.emulator.read_pixel(x, y));
                painter.rect_filled(
                    rect_for_coordinate(x, y, scale_factor),
                    CornerRadius::ZERO,
                    color32,
                );
            }
        }
    }

    fn draw_frame_rate_counter(&self, ui: &mut egui::Ui) {
        let fps = self.emulator.current_frame_rate();

        ui.painter().text(
            Pos2::new(4.0, 4.0),
            Align2::LEFT_TOP,
            fps.to_string(),
            FontId::monospace(24.0),
            FPS_COUNTER_COLOR,
        );
    }

    fn calculate_scale_factor(&self, ctx: &egui::Context) -> f32 {
        let viewport_rect = ctx.viewport_rect();

        let width_scale = viewport_rect.width() / (SCREEN_WIDTH as f32);
        let height_scale = viewport_rect.height() / (SCREEN_HEIGHT as f32);

        width_scale.min(height_scale)
    }

    pub fn resize_to_fit(&self, ctx: &egui::Context) {
        let scale_factor = self.calculate_scale_factor(ctx);

        let new_size = Vec2::new(
            scale_factor * (SCREEN_WIDTH as f32),
            scale_factor * (SCREEN_HEIGHT as f32),
        );

        ctx.send_viewport_cmd(ViewportCommand::InnerSize(new_size));
    }

    pub fn toggle_show_fps(&mut self) {
        self.show_fps = !self.show_fps;
    }

    pub fn show_debugger_view(&mut self, ctx: &egui::Context) {
        if self.debugger_view().is_shown() {
            return;
        }

        let initial_position =
            self.additional_viewport_initial_position(ctx, DEBUGGER_WINDOW_INNER_SIZE);
        self.debugger_view_mut().open(initial_position);
    }

    pub fn show_vram_view(&mut self, ctx: &egui::Context) {
        if self.vram_view().is_shown() {
            return;
        }

        let initial_position =
            self.additional_viewport_initial_position(ctx, self.vram_viewport_size());
        self.vram_view_mut().open(initial_position);
    }

    pub fn vram_view(&self) -> &VramViewport {
        &self.vram_view
    }

    pub fn vram_view_mut(&mut self) -> &mut VramViewport {
        &mut self.vram_view
    }

    pub fn debugger_view(&self) -> &DebuggerViewport {
        &self.debugger_view
    }

    pub fn debugger_view_mut(&mut self) -> &mut DebuggerViewport {
        &mut self.debugger_view
    }

    /// Outer bounds of the root emulator viewport
    fn emulator_viewport_outer_rect(&self, ctx: &egui::Context) -> egui::Rect {
        ctx.viewport_for(egui::ViewportId::ROOT, |viewport| {
            viewport.input.viewport().outer_rect.unwrap()
        })
    }

    /// Initial position for additional viewports (e.g. VRAM view, debugger)
    ///
    /// Additional viewports are positioned to the left of the main emulator viewport.
    pub fn additional_viewport_initial_position(
        &self,
        ctx: &egui::Context,
        viewport_size: Vec2,
    ) -> Pos2 {
        const LEFT_MARGIN: f32 = 20.0;

        let root_rect = self.emulator_viewport_outer_rect(ctx);

        Pos2::new(
            (root_rect.left() - viewport_size.x - LEFT_MARGIN).max(0.0),
            (root_rect.center().y - (viewport_size.y / 2.0)).max(0.0),
        )
    }

    fn handle_window_close_events(&mut self, ctx: &egui::Context) {
        ctx.viewport_for(self.vram_viewport_id(), |viewport| {
            if viewport.input.viewport().close_requested() {
                self.vram_view.close();
            }
        });

        ctx.viewport_for(self.debugger_viewport_id(), |viewport| {
            if viewport.input.viewport().close_requested() {
                self.debugger_view.close();
            }
        });
    }
}

impl eframe::App for EmulatorShellApp {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        Color32::TRANSPARENT.to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if !self.is_initialized {
            self.init(ctx);
        }

        ctx.request_repaint_after(Duration::from_secs_f64(1.0 / GUI_FPS));

        self.handle_menu_events(ctx);
        self.handle_pressed_buttons(ctx);
        self.handle_turbo_mode(ctx);
        self.handle_window_close_events(ctx);

        self.draw(ctx);
    }
}

const FPS_COUNTER_COLOR: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 255, 128);
