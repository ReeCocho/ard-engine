use ard_math::{IVec2, Vec2};
use winit::{
    dpi::{LogicalPosition, Position},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
    window::{CursorGrabMode, CursorIcon},
};

pub struct Window {
    pub(crate) winit_window: winit::window::Window,
    window_handle: RawWindowHandle,
    display_handle: RawDisplayHandle,
    requested_width: f32,
    requested_height: f32,
    physical_width: u32,
    physical_height: u32,
    resize_constraints: WindowResizeConstraints,
    position: Option<IVec2>,
    scale_factor_override: Option<f64>,
    backend_scale_factor: f64,
    title: String,
    vsync: bool,
    resizable: bool,
    decorations: bool,
    cursor_visible: bool,
    cursor_locked: bool,
    cursor_position: Option<Vec2>,
    cursor_icon: CursorIcon,
    focused: bool,
    mode: WindowMode,
    command_queue: Vec<WindowCommand>,
}

#[derive(Debug, Clone)]
pub struct WindowDescriptor {
    pub width: f32,
    pub height: f32,
    pub resize_constraints: WindowResizeConstraints,
    pub scale_factor_override: Option<f64>,
    pub title: String,
    pub vsync: bool,
    pub resizable: bool,
    pub decorations: bool,
    pub cursor_visible: bool,
    pub cursor_locked: bool,
    pub mode: WindowMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(usize);

/// The size limits on a window.
/// These values are measured in logical pixels, so the user's
/// scale factor does affect the size limits on the window.
/// Please note that if the window is resizable, then when the window is
/// maximized it may have a size outside of these limits. The functionality
/// required to disable maximizing is not yet exposed by winit.
#[derive(Debug, Clone, Copy)]
pub struct WindowResizeConstraints {
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: f32,
    pub max_height: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub enum WindowMode {
    Windowed,
    BorderlessFullscreen,
    Fullscreen { use_size: bool },
}

#[derive(Debug)]
pub enum WindowCommand {
    SetWindowMode {
        mode: WindowMode,
        resolution: (u32, u32),
    },
    SetTitle {
        title: String,
    },
    SetScaleFactor {
        scale_factor: f64,
    },
    SetResolution {
        logical_resolution: (f32, f32),
        scale_factor: f64,
    },
    SetVsync {
        vsync: bool,
    },
    SetResizable {
        resizable: bool,
    },
    SetDecorations {
        decorations: bool,
    },
    SetCursorLockMode {
        locked: bool,
    },
    SetCursorVisibility {
        visible: bool,
    },
    SetCursorPosition {
        position: Vec2,
    },
    SetCursor {
        icon: CursorIcon,
    },
    SetMaximized {
        maximized: bool,
    },
    SetMinimized {
        minimized: bool,
    },
    SetPosition {
        position: IVec2,
    },
    SetResizeConstraints {
        resize_constraints: WindowResizeConstraints,
    },
}

impl WindowId {
    pub const fn new(id_raw: usize) -> Self {
        WindowId(id_raw)
    }

    pub const fn primary() -> Self {
        WindowId(usize::MAX)
    }

    pub const fn is_primary(&self) -> bool {
        self.0 == WindowId::primary().0
    }
}

impl Default for WindowDescriptor {
    fn default() -> Self {
        WindowDescriptor {
            title: String::from("Window"),
            width: 800.0,
            height: 600.0,
            resize_constraints: WindowResizeConstraints::default(),
            scale_factor_override: None,
            vsync: true,
            resizable: true,
            decorations: true,
            cursor_locked: false,
            cursor_visible: true,
            mode: WindowMode::Windowed,
        }
    }
}

impl Window {
    pub fn new(winit_window: winit::window::Window, window_descriptor: &WindowDescriptor) -> Self {
        Window {
            requested_width: window_descriptor.width,
            requested_height: window_descriptor.height,
            position: winit_window
                .outer_position()
                .ok()
                .map(|position| IVec2::new(position.x, position.y)),
            physical_width: winit_window.inner_size().width,
            physical_height: winit_window.inner_size().height,
            resize_constraints: window_descriptor.resize_constraints,
            scale_factor_override: window_descriptor.scale_factor_override,
            backend_scale_factor: winit_window.scale_factor(),
            title: window_descriptor.title.clone(),
            vsync: window_descriptor.vsync,
            resizable: window_descriptor.resizable,
            decorations: window_descriptor.decorations,
            cursor_visible: window_descriptor.cursor_visible,
            cursor_locked: window_descriptor.cursor_locked,
            cursor_position: None,
            cursor_icon: CursorIcon::Default,
            focused: true,
            mode: window_descriptor.mode,
            command_queue: Vec::new(),
            window_handle: winit_window.window_handle().unwrap().as_raw(),
            display_handle: winit_window.display_handle().unwrap().as_raw(),
            winit_window,
        }
    }

    #[inline(always)]
    pub fn window_handle(&self) -> RawWindowHandle {
        self.window_handle
    }

    #[inline(always)]
    pub fn display_handle(&self) -> RawDisplayHandle {
        self.display_handle
    }

    /// The current logical width of the window's client area.
    #[inline]
    pub fn width(&self) -> f32 {
        (self.physical_width as f64 / self.scale_factor()) as f32
    }

    /// The current logical height of the window's client area.
    #[inline]
    pub fn height(&self) -> f32 {
        (self.physical_height as f64 / self.scale_factor()) as f32
    }

    /// The requested window client area width in logical pixels from window
    /// creation or the last call to [set_resolution](Window::set_resolution).
    ///
    /// This may differ from the actual width depending on OS size limits and
    /// the scaling factor for high DPI monitors.
    #[inline]
    pub fn requested_width(&self) -> f32 {
        self.requested_width
    }

    /// The requested window client area height in logical pixels from window
    /// creation or the last call to [set_resolution](Window::set_resolution).
    ///
    /// This may differ from the actual width depending on OS size limits and
    /// the scaling factor for high DPI monitors.
    #[inline]
    pub fn requested_height(&self) -> f32 {
        self.requested_height
    }

    /// The window's client area width in physical pixels.
    #[inline]
    pub fn physical_width(&self) -> u32 {
        self.physical_width
    }

    /// The window's client area height in physical pixels.
    #[inline]
    pub fn physical_height(&self) -> u32 {
        self.physical_height
    }

    /// The window's client resize constraint in logical pixels.
    #[inline]
    pub fn resize_constraints(&self) -> WindowResizeConstraints {
        self.resize_constraints
    }

    /// The window's client position in physical pixels.
    #[inline]
    pub fn position(&self) -> Option<IVec2> {
        self.position
    }

    #[inline]
    pub fn set_maximized(&mut self, maximized: bool) {
        self.command_queue
            .push(WindowCommand::SetMaximized { maximized });
    }

    /// Sets the window to minimized or back.
    #[inline]
    pub fn set_minimized(&mut self, minimized: bool) {
        self.command_queue
            .push(WindowCommand::SetMinimized { minimized });
    }

    /// Modifies the position of the window in physical pixels.
    ///
    /// Note that the top-left hand corner of the desktop is not necessarily the same as the screen.
    /// If the user uses a desktop with multiple monitors, the top-left hand corner of the
    /// desktop is the top-left hand corner of the monitor at the top-left of the desktop. This
    /// automatically un-maximizes the window if it's maximized.
    #[inline]
    pub fn set_position(&mut self, position: IVec2) {
        self.command_queue
            .push(WindowCommand::SetPosition { position })
    }

    /// Modifies the minimum and maximum window bounds for resizing in logical pixels.
    #[inline]
    pub fn set_resize_constraints(&mut self, resize_constraints: WindowResizeConstraints) {
        self.command_queue
            .push(WindowCommand::SetResizeConstraints { resize_constraints });
    }

    /// Request the OS to resize the window such the the client area matches the
    /// specified width and height.
    #[allow(clippy::float_cmp)]
    pub fn set_resolution(&mut self, width: f32, height: f32) {
        if self.requested_width == width && self.requested_height == height {
            return;
        }

        self.requested_width = width;
        self.requested_height = height;
        self.command_queue.push(WindowCommand::SetResolution {
            logical_resolution: (self.requested_width, self.requested_height),
            scale_factor: self.scale_factor(),
        });
    }

    /// Override the os-reported scaling factor
    #[allow(clippy::float_cmp)]
    pub fn set_scale_factor_override(&mut self, scale_factor: Option<f64>) {
        if self.scale_factor_override == scale_factor {
            return;
        }

        self.scale_factor_override = scale_factor;
        self.command_queue.push(WindowCommand::SetScaleFactor {
            scale_factor: self.scale_factor(),
        });
        self.command_queue.push(WindowCommand::SetResolution {
            logical_resolution: (self.requested_width, self.requested_height),
            scale_factor: self.scale_factor(),
        });
    }

    #[inline]
    pub fn update_scale_factor_from_backend(&mut self, scale_factor: f64) {
        self.backend_scale_factor = scale_factor;
    }

    #[inline]
    pub fn update_actual_size_from_backend(&mut self, physical_width: u32, physical_height: u32) {
        self.physical_width = physical_width;
        self.physical_height = physical_height;
    }

    #[inline]
    pub fn update_actual_position_from_backend(&mut self, position: IVec2) {
        self.position = Some(position);
    }

    /// The ratio of physical pixels to logical pixels
    ///
    /// `physical_pixels = logical_pixels * scale_factor`
    pub fn scale_factor(&self) -> f64 {
        self.scale_factor_override
            .unwrap_or(self.backend_scale_factor)
    }

    /// The window scale factor as reported by the window backend.
    /// This value is unaffected by scale_factor_override.
    #[inline]
    pub fn backend_scale_factor(&self) -> f64 {
        self.backend_scale_factor
    }

    #[inline]
    pub fn scale_factor_override(&self) -> Option<f64> {
        self.scale_factor_override
    }

    #[inline]
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: String) {
        self.title = title.to_string();
        self.command_queue.push(WindowCommand::SetTitle { title });
    }

    #[inline]
    pub fn vsync(&self) -> bool {
        self.vsync
    }

    #[inline]
    pub fn set_vsync(&mut self, vsync: bool) {
        self.vsync = vsync;
        self.command_queue.push(WindowCommand::SetVsync { vsync });
    }

    #[inline]
    pub fn resizable(&self) -> bool {
        self.resizable
    }

    pub fn set_resizable(&mut self, resizable: bool) {
        self.resizable = resizable;
        self.command_queue
            .push(WindowCommand::SetResizable { resizable });
    }

    #[inline]
    pub fn decorations(&self) -> bool {
        self.decorations
    }

    pub fn set_decorations(&mut self, decorations: bool) {
        self.decorations = decorations;
        self.command_queue
            .push(WindowCommand::SetDecorations { decorations });
    }

    #[inline]
    pub fn cursor_locked(&self) -> bool {
        self.cursor_locked
    }

    pub fn set_cursor_lock_mode(&mut self, lock_mode: bool) {
        self.cursor_locked = lock_mode;
        self.command_queue
            .push(WindowCommand::SetCursorLockMode { locked: lock_mode });
    }

    #[inline]
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn set_cursor_visibility(&mut self, visibile_mode: bool) {
        self.cursor_visible = visibile_mode;
        self.command_queue.push(WindowCommand::SetCursorVisibility {
            visible: visibile_mode,
        });
    }

    #[inline]
    pub fn cursor_position(&self) -> Option<Vec2> {
        self.cursor_position
    }

    pub fn set_cursor_position(&mut self, position: Vec2) {
        self.command_queue
            .push(WindowCommand::SetCursorPosition { position });
    }

    #[inline]
    pub fn cursor_icon(&self) -> CursorIcon {
        self.cursor_icon
    }

    pub fn set_cursor_icon(&mut self, icon: CursorIcon) {
        self.command_queue.push(WindowCommand::SetCursor { icon });
    }

    #[inline]
    pub fn update_focused_status_from_backend(&mut self, focused: bool) {
        self.focused = focused;
    }

    #[inline]
    pub fn update_cursor_position_from_backend(&mut self, cursor_position: Option<Vec2>) {
        self.cursor_position = cursor_position;
    }

    #[inline]
    pub fn mode(&self) -> WindowMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: WindowMode) {
        self.mode = mode;
        self.command_queue.push(WindowCommand::SetWindowMode {
            mode,
            resolution: (self.physical_width, self.physical_height),
        });
    }

    #[inline]
    pub fn apply_commands(&mut self) {
        for command in self.command_queue.drain(..) {
            match command {
                WindowCommand::SetWindowMode { .. } => todo!(),
                WindowCommand::SetTitle { title } => self.winit_window.set_title(title.as_str()),
                WindowCommand::SetScaleFactor { .. } => todo!(),
                WindowCommand::SetResolution { .. } => todo!(),
                WindowCommand::SetVsync { .. } => todo!(),
                WindowCommand::SetResizable { resizable } => {
                    self.winit_window.set_resizable(resizable)
                }
                WindowCommand::SetDecorations { decorations } => {
                    self.winit_window.set_decorations(decorations)
                }
                WindowCommand::SetCursorLockMode { locked } => self
                    .winit_window
                    .set_cursor_grab(if locked {
                        if cfg!(target_os = "macos") {
                            CursorGrabMode::Locked
                        } else {
                            CursorGrabMode::Confined
                        }
                    } else {
                        CursorGrabMode::None
                    })
                    .unwrap(),
                WindowCommand::SetCursorVisibility { visible } => {
                    self.winit_window.set_cursor_visible(visible)
                }
                WindowCommand::SetCursorPosition { position } => self
                    .winit_window
                    .set_cursor_position(Position::Logical(LogicalPosition::new(
                        position.x as f64,
                        position.y as f64,
                    )))
                    .unwrap(),
                WindowCommand::SetCursor { icon } => self.winit_window.set_cursor(icon),
                WindowCommand::SetMaximized { maximized } => {
                    self.winit_window.set_maximized(maximized)
                }
                WindowCommand::SetMinimized { minimized } => {
                    self.winit_window.set_minimized(minimized)
                }
                WindowCommand::SetPosition { .. } => todo!(),
                WindowCommand::SetResizeConstraints { .. } => {
                    todo!()
                }
            }
        }
    }

    #[inline]
    pub fn is_focused(&self) -> bool {
        self.focused
    }
}

impl Default for WindowResizeConstraints {
    fn default() -> Self {
        Self {
            min_width: 180.,
            min_height: 120.,
            max_width: f32::INFINITY,
            max_height: f32::INFINITY,
        }
    }
}

impl WindowResizeConstraints {
    pub fn check_constraints(&self) -> WindowResizeConstraints {
        let WindowResizeConstraints {
            mut min_width,
            mut min_height,
            mut max_width,
            mut max_height,
        } = self;
        min_width = min_width.max(1.);
        min_height = min_height.max(1.);
        if max_width < min_width {
            max_width = min_width;
        }
        if max_height < min_height {
            max_height = min_height;
        }
        WindowResizeConstraints {
            min_width,
            min_height,
            max_width,
            max_height,
        }
    }
}
