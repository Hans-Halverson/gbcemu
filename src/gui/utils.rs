use eframe::egui::Rect;

pub fn rect_for_coordinate(x: usize, y: usize, scale_factor: f32) -> Rect {
    Rect::from_x_y_ranges(
        ((x as f32) * scale_factor)..=(((x as f32) + 1.0) * scale_factor),
        ((y as f32) * scale_factor)..=(((y as f32) + 1.0) * scale_factor),
    )
}
