use std::collections::HashMap;

use ard_ecs::prelude::*;
use ard_math::IVec2;
use ard_window::prelude::{WindowDescriptor, WindowMode};
use winit::{dpi::LogicalSize, event_loop::EventLoopWindowTarget, window::CursorGrabMode};

use crate::{Window, WindowId};

#[derive(Debug, Default, Resource)]
pub struct WinitWindows {
    windows: HashMap<winit::window::WindowId, winit::window::Window>,
    window_id_to_winit: HashMap<WindowId, winit::window::WindowId>,
    winit_to_window_id: HashMap<winit::window::WindowId, WindowId>,
}

impl WinitWindows {
    /// Manually create a new window from its ID and descriptor.
    pub fn create_window(
        &mut self,
        event_loop: &EventLoopWindowTarget<()>,
        id: WindowId,
        descriptor: WindowDescriptor,
    ) -> Window {
        let mut builder = winit::window::WindowBuilder::new();

        builder = match descriptor.mode {
            WindowMode::BorderlessFullscreen => builder.with_fullscreen(Some(
                winit::window::Fullscreen::Borderless(event_loop.primary_monitor()),
            )),
            WindowMode::Fullscreen { use_size } => builder.with_fullscreen(Some(
                winit::window::Fullscreen::Exclusive(match use_size {
                    true => get_fitting_videomode(
                        &event_loop.primary_monitor().unwrap(),
                        descriptor.width as u32,
                        descriptor.height as u32,
                    ),
                    false => get_best_videomode(&event_loop.primary_monitor().unwrap()),
                }),
            )),
            _ => {
                let WindowDescriptor {
                    width,
                    height,
                    scale_factor_override,
                    ..
                } = &descriptor;
                if let Some(sf) = scale_factor_override {
                    builder.with_inner_size(
                        winit::dpi::LogicalSize::new(*width, *height).to_physical::<f64>(*sf),
                    )
                } else {
                    builder.with_inner_size(winit::dpi::LogicalSize::new(*width, *height))
                }
            }
            .with_resizable(descriptor.resizable)
            .with_decorations(descriptor.decorations),
        };

        let constraints = descriptor.resize_constraints.check_constraints();
        let min_inner_size = LogicalSize {
            width: constraints.min_width,
            height: constraints.min_height,
        };
        let max_inner_size = LogicalSize {
            width: constraints.max_width,
            height: constraints.max_height,
        };

        let winit_window_builder =
            if constraints.max_width.is_finite() && constraints.max_height.is_finite() {
                builder
                    .with_min_inner_size(min_inner_size)
                    .with_max_inner_size(max_inner_size)
            } else {
                builder.with_min_inner_size(min_inner_size)
            };

        let winit_window_builder = winit_window_builder.with_title(&descriptor.title);

        let winit_window = winit_window_builder.build(event_loop).unwrap();

        match winit_window.set_cursor_grab(if descriptor.cursor_locked {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        }) {
            Ok(_) => {}
            Err(winit::error::ExternalError::NotSupported(_)) => {}
            Err(err) => panic!("{:?}", err),
        }

        winit_window.set_cursor_visible(descriptor.cursor_visible);

        self.window_id_to_winit.insert(id, winit_window.id());
        self.winit_to_window_id.insert(winit_window.id(), id);

        let position = winit_window
            .outer_position()
            .ok()
            .map(|position| IVec2::new(position.x, position.y));
        let inner_size = winit_window.inner_size();
        let scale_factor = winit_window.scale_factor();
        self.windows.insert(winit_window.id(), winit_window);

        Window::new(
            id,
            &descriptor,
            inner_size.width,
            inner_size.height,
            scale_factor,
            position,
        )
    }

    pub fn get_window(&self, id: WindowId) -> Option<&winit::window::Window> {
        self.window_id_to_winit
            .get(&id)
            .and_then(|id| self.windows.get(id))
    }

    pub fn get_window_id(&self, id: winit::window::WindowId) -> Option<WindowId> {
        self.winit_to_window_id.get(&id).cloned()
    }
}

pub fn get_fitting_videomode(
    monitor: &winit::monitor::MonitorHandle,
    width: u32,
    height: u32,
) -> winit::monitor::VideoMode {
    let mut modes = monitor.video_modes().collect::<Vec<_>>();

    fn abs_diff(a: u32, b: u32) -> u32 {
        if a > b {
            return a - b;
        }
        b - a
    }

    modes.sort_by(|a, b| {
        use std::cmp::Ordering::*;
        match abs_diff(a.size().width, width).cmp(&abs_diff(b.size().width, width)) {
            Equal => {
                match abs_diff(a.size().height, height).cmp(&abs_diff(b.size().height, height)) {
                    Equal => b
                        .refresh_rate_millihertz()
                        .cmp(&a.refresh_rate_millihertz()),
                    default => default,
                }
            }
            default => default,
        }
    });

    modes.first().unwrap().clone()
}

pub fn get_best_videomode(monitor: &winit::monitor::MonitorHandle) -> winit::monitor::VideoMode {
    let mut modes = monitor.video_modes().collect::<Vec<_>>();
    modes.sort_by(|a, b| {
        use std::cmp::Ordering::*;
        match b.size().width.cmp(&a.size().width) {
            Equal => match b.size().height.cmp(&a.size().height) {
                Equal => b
                    .refresh_rate_millihertz()
                    .cmp(&a.refresh_rate_millihertz()),
                default => default,
            },
            default => default,
        }
    });

    modes.first().unwrap().clone()
}
