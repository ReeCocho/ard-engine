use ard_ecs::prelude::*;
use rustc_hash::FxHashMap;
use winit::{
    dpi::LogicalSize,
    event_loop::{ActiveEventLoop, OwnedDisplayHandle},
    window::{CursorGrabMode, Fullscreen, WindowAttributes},
};

use crate::{
    prelude::WindowId,
    window::{Window, WindowDescriptor, WindowMode},
};

#[derive(Resource)]
pub struct Windows {
    windows: FxHashMap<WindowId, Window>,
    winit_to_ard: FxHashMap<winit::window::WindowId, WindowId>,
    to_create: Vec<PendingWindow>,
    display_handle: OwnedDisplayHandle,
}

pub(crate) struct PendingWindow {
    pub id: WindowId,
    pub descriptor: WindowDescriptor,
}

impl Windows {
    pub fn new(display_handle: OwnedDisplayHandle) -> Self {
        Self {
            windows: FxHashMap::default(),
            winit_to_ard: FxHashMap::default(),
            to_create: Vec::default(),
            display_handle,
        }
    }

    #[inline(always)]
    pub fn display_handle(&self) -> &OwnedDisplayHandle {
        &self.display_handle
    }

    #[inline]
    pub fn create(&mut self, id: WindowId, descriptor: WindowDescriptor) {
        assert!(
            !self.windows.contains_key(&id),
            "Window ID must not be in use."
        );
        self.to_create.push(PendingWindow { id, descriptor });
    }

    #[inline]
    pub fn winit_to_ard_id(&self, id: winit::window::WindowId) -> Option<WindowId> {
        self.winit_to_ard.get(&id).copied()
    }

    #[inline]
    pub fn get(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(&id)
    }

    #[inline]
    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.get_mut(&id)
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Window> {
        self.windows.values()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Window> {
        self.windows.values_mut()
    }

    pub(crate) fn add_pending(&mut self, event_loop: &ActiveEventLoop) {
        self.to_create.drain(..).for_each(|pending| {
            assert!(
                !self.windows.contains_key(&pending.id),
                "Window ID must not be in use."
            );

            let constraints = pending.descriptor.resize_constraints.check_constraints();
            let min_inner_size = LogicalSize {
                width: constraints.min_width as f64,
                height: constraints.min_height as f64,
            };
            let max_inner_size = LogicalSize {
                width: constraints.max_width as f64,
                height: constraints.max_height as f64,
            };

            let fullscreen = match pending.descriptor.mode {
                WindowMode::Windowed => None,
                WindowMode::BorderlessFullscreen => Some(Fullscreen::Borderless(None)),
                WindowMode::Fullscreen { use_size } => Some(Fullscreen::Exclusive(if use_size {
                    get_fitting_videomode(
                        &event_loop.primary_monitor().unwrap(),
                        pending.descriptor.width as u32,
                        pending.descriptor.height as u32,
                    )
                } else {
                    get_best_videomode(&event_loop.primary_monitor().unwrap())
                })),
            };

            let mut attributes = WindowAttributes::default();
            attributes.inner_size = match pending.descriptor.mode {
                WindowMode::Windowed => {
                    if let Some(sf) = pending.descriptor.scale_factor_override {
                        Some(winit::dpi::Size::Physical(
                            winit::dpi::LogicalSize::new(
                                pending.descriptor.width as u32,
                                pending.descriptor.height as u32,
                            )
                            .to_physical::<u32>(sf),
                        ))
                    } else {
                        Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                            pending.descriptor.width as f64,
                            pending.descriptor.height as f64,
                        )))
                    }
                }
                _ => None,
            };
            attributes.min_inner_size = Some(winit::dpi::Size::Logical(min_inner_size));
            if constraints.max_width.is_finite() && constraints.max_height.is_finite() {
                attributes.max_inner_size = Some(winit::dpi::Size::Logical(max_inner_size));
            }
            attributes.resizable = pending.descriptor.resizable;
            attributes.title = pending.descriptor.title.clone();
            attributes.decorations = pending.descriptor.decorations;
            attributes.fullscreen = fullscreen;

            let window = event_loop.create_window(attributes).unwrap();
            window.set_cursor_visible(pending.descriptor.cursor_visible);
            window
                .set_cursor_grab(if pending.descriptor.cursor_locked {
                    CursorGrabMode::Locked
                } else {
                    CursorGrabMode::None
                })
                .unwrap();

            self.winit_to_ard.insert(window.id(), pending.id);
            self.windows
                .insert(pending.id, Window::new(window, &pending.descriptor));
        });
    }
}

pub fn get_fitting_videomode(
    monitor: &winit::monitor::MonitorHandle,
    width: u32,
    height: u32,
) -> winit::monitor::VideoModeHandle {
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

pub fn get_best_videomode(
    monitor: &winit::monitor::MonitorHandle,
) -> winit::monitor::VideoModeHandle {
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
