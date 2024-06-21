use egui::{Color32, WidgetText};

pub const DESTRUCTIVE_COLOR: Color32 = Color32::from_rgb(255, 0, 125);
pub const CONSTRUCTIVE_COLOR: Color32 = Color32::from_rgb(0, 255, 125);
pub const TRANSFORMATIVE_COLOR: Color32 = Color32::from_rgb(0, 125, 255);

pub fn destructive_button<'a>(text: impl Into<WidgetText>) -> egui::Button<'a> {
    egui::Button::new(text).fill(DESTRUCTIVE_COLOR)
}

pub fn constructive_button<'a>(text: impl Into<String>) -> egui::Button<'a> {
    egui::Button::new(
        egui::RichText::new(text)
            .color(Color32::BLACK)
            .text_style(egui::TextStyle::Button),
    )
    .fill(CONSTRUCTIVE_COLOR)
}

pub fn transformation_button<'a>(text: impl Into<String>) -> egui::Button<'a> {
    egui::Button::new(
        egui::RichText::new(text)
            .color(Color32::WHITE)
            .text_style(egui::TextStyle::Button),
    )
    .fill(TRANSFORMATIVE_COLOR)
}
