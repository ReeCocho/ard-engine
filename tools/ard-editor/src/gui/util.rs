use std::{any::Any, sync::Arc};

use egui::{Color32, WidgetText};

use crate::assets::EditorAsset;

use super::drag_drop::DragDropPayload;

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

pub fn hidden_drop_zone<Payload, R>(
    ui: &mut egui::Ui,
    frame: egui::Frame,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> (egui::InnerResponse<R>, Option<Arc<Payload>>)
where
    Payload: Any + Send + Sync,
{
    let mut frame = frame.begin(ui);
    let inner = add_contents(&mut frame.content_ui);
    let response = frame.allocate_space(ui);

    // frame.frame.fill = fill;
    // frame.frame.stroke = stroke;

    frame.paint(ui);

    let payload = response.dnd_release_payload::<Payload>();

    (egui::InnerResponse { inner, response }, payload)
}

pub fn drag_drop_asset_target(
    ui: &mut egui::Ui,
    label: impl AsRef<str>,
    is_valid: impl FnOnce(&EditorAsset) -> bool,
    mut apply: impl FnMut(Option<&EditorAsset>) -> bool,
) -> bool {
    let mut changed = false;

    let payload = ui
        .horizontal(|ui| {
            let (_, payload) =
                ui.dnd_drop_zone::<DragDropPayload, _>(egui::Frame::default(), |ui| {
                    let mut label = label.as_ref().to_owned();
                    ui.add_enabled(false, egui::TextEdit::singleline(&mut label));
                });

            if ui.button("Clear").clicked() {
                changed = apply(None);
            }

            payload
        })
        .inner;

    if changed {
        return true;
    }

    let payload = match payload {
        Some(payload) => payload,
        None => return false,
    };

    let asset = match payload.as_ref() {
        DragDropPayload::Asset(asset) => asset,
        _ => return false,
    };

    if !is_valid(asset) {
        return false;
    }

    apply(Some(asset))
}
