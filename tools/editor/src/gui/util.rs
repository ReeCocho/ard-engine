use ard_engine::{assets::prelude::RawHandle, ecs::prelude::Entity};

#[derive(Debug, Copy, Clone)]
pub enum DragDropPayload {
    Entity(Entity),
    Asset(RawHandle),
}

pub fn throbber(
    ui: &imgui::Ui,
    radius: f32,
    thickness: f32,
    num_segments: i32,
    speed: f32,
    color: impl Into<imgui::ImColor32>,
) {
    let mut pos = ui.cursor_pos();
    let wpos = ui.window_pos();
    pos[0] += wpos[0];
    pos[1] += wpos[1];

    let size = [radius * 2.0, radius * 2.0];

    let rect = imgui::sys::ImRect {
        Min: imgui::sys::ImVec2::new(pos[0] - thickness, pos[1] - thickness),
        Max: imgui::sys::ImVec2::new(pos[0] + size[0] + thickness, pos[1] + size[1] + thickness),
    };

    unsafe {
        imgui::sys::igItemSizeRect(rect, 0.0);

        if !imgui::sys::igItemAdd(
            rect,
            0,
            std::ptr::null(),
            imgui::sys::ImGuiItemFlags_None as i32,
        ) {
            return;
        }
    }

    let time = ui.time() as f32 * speed;

    let start = (time.sin() * (num_segments - 5) as f32).abs() as i32;
    let min = 2.0 * std::f32::consts::PI * (start as f32 / num_segments as f32);
    let max = 2.0 * std::f32::consts::PI * ((num_segments - 3) as f32 / num_segments as f32);
    let center = [pos[0] + radius, pos[1] + radius];

    let mut points = Vec::with_capacity(num_segments as usize);

    for i in 0..num_segments {
        let a = min + (i as f32 / num_segments as f32) * (max - min);
        let x = (a + time * 8.0).cos() * radius;
        let y = (a + time * 8.0).sin() * radius;
        let new_pos = [center[0] + x, center[1] + y];

        points.push(new_pos);
    }

    // NOTE: Polyline is supposed to be in window coordinates, but for whatever reason it is
    // actually in screen coordinates here. If the throbber ever bugs out, check this first.
    ui.get_window_draw_list()
        .add_polyline(points, color.into())
        .thickness(thickness)
        .build();
}
