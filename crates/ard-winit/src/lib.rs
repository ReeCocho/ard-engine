pub mod windows;

pub mod prelude {
    pub use crate::windows::*;
    pub use crate::*;
}

use std::{cmp::Ordering, time::Instant};

use ard_core::prelude::*;
use ard_input::{InputState, Key, MouseButton};
use ard_window::prelude::*;

use prelude::WinitWindows;
use winit::{
    dpi::{LogicalPosition, Position},
    event::{Event, TouchPhase, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget},
    platform::run_return::EventLoopExtRunReturn,
    window::CursorGrabMode,
};

/// Plugin that adds winit integration
#[derive(Debug, Default)]
pub struct WinitPlugin;

#[derive(Debug, Default)]
pub struct WinitSystem;

impl Plugin for WinitPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        app.add_resource(windows::WinitWindows::default());
        app.add_resource(InputState::default());
        app.with_runner(winit_runner);
    }
}

fn winit_runner(mut app: App) {
    let mut event_loop = EventLoop::new();

    // Create initial windows if requested
    {
        let mut windows = app.resources.get_mut::<Windows>().unwrap();
        let mut winit_windows = app.resources.get_mut::<windows::WinitWindows>().unwrap();
        create_windows(&event_loop, &mut windows, &mut winit_windows);
    }

    // Run startup functions
    app.run_startups();

    let mut dispatcher = std::mem::take(&mut app.dispatcher).build();

    // Begin main loop
    let mut last = Instant::now();
    event_loop.run_return(|event, event_loop, control_flow| {
        *control_flow = ControlFlow::Poll;

        let mut windows = app.resources.get_mut::<Windows>().unwrap();
        let mut winit_windows = app.resources.get_mut::<windows::WinitWindows>().unwrap();
        let mut input = app.resources.get_mut::<InputState>().unwrap();

        match event {
            Event::WindowEvent { window_id, event } => {
                // Get the Ard ID of the window
                let ard_id = winit_windows
                    .get_window_id(window_id)
                    .expect("winit window did not point to ard window");

                let window = windows.get_mut(ard_id).unwrap();

                // Dispatch event
                match event {
                    WindowEvent::CloseRequested => dispatcher.submit(WindowClosed(ard_id)),
                    WindowEvent::CursorMoved { position, .. } => {
                        input.signal_mouse_pos((position.x, position.y));
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        if let Some(button) = winit_to_ard_mouse_button(button) {
                            match state {
                                winit::event::ElementState::Pressed => {
                                    input.signal_mouse_button_down(button)
                                }
                                winit::event::ElementState::Released => {
                                    input.signal_mouse_button_up(button)
                                }
                            }
                        }
                    }
                    WindowEvent::DroppedFile(path) => {
                        dispatcher.event_sender().submit(WindowFileDropped {
                            window: ard_id,
                            file: path,
                        });
                    }
                    WindowEvent::MouseWheel {
                        delta,
                        phase: TouchPhase::Moved,
                        ..
                    } => match delta {
                        winit::event::MouseScrollDelta::LineDelta(h, v) => {
                            input.signal_scroll((h as f64, v as f64));
                        }
                        winit::event::MouseScrollDelta::PixelDelta(pos) => {
                            let pos = pos.to_logical::<f64>(window.scale_factor());
                            match pos.x.partial_cmp(&0.0) {
                                Some(Ordering::Greater) => input.signal_scroll((1.0, 0.0)),
                                Some(Ordering::Less) => input.signal_scroll((-1.0, 0.0)),
                                _ => (),
                            }
                            match pos.y.partial_cmp(&0.0) {
                                Some(Ordering::Greater) => input.signal_scroll((0.0, 1.0)),
                                Some(Ordering::Less) => input.signal_scroll((0.0, -1.0)),
                                _ => (),
                            }
                        }
                    },
                    WindowEvent::Resized(dims) => {
                        dispatcher.submit(WindowResized {
                            id: ard_id,
                            width: dims.width,
                            height: dims.height,
                        });
                        window.update_actual_size_from_backend(dims.width, dims.height)
                    }
                    WindowEvent::ReceivedCharacter(ch) => {
                        // Exclude the backspace key ('\u{7f}'). Otherwise we will insert this char and then
                        // delete it.
                        if ch != '\u{7f}' {
                            input.signal_character(ch);
                        }
                    }
                    _ => {}
                }
            }
            Event::DeviceEvent { event, .. } => match event {
                winit::event::DeviceEvent::Key(key) => {
                    if let Some(virtual_keycode) = key.virtual_keycode {
                        if let Some(ard_key) = winit_to_ard_key(virtual_keycode) {
                            match key.state {
                                winit::event::ElementState::Pressed => {
                                    input.signal_key_down(ard_key)
                                }
                                winit::event::ElementState::Released => {
                                    input.signal_key_up(ard_key)
                                }
                            }
                        }
                    }
                }
                winit::event::DeviceEvent::MouseMotion { delta } => {
                    input.signal_mouse_movement(delta);
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                // Check if `Stop` was requested
                if app.resources.get::<ArdCoreState>().unwrap().stopping() {
                    *control_flow = ControlFlow::Exit;

                    // Handle `Stopping` event
                    dispatcher.run(&mut app.world, &app.resources);
                } else {
                    // Create windows if needed and request redraw from the primary window
                    create_windows(event_loop, &mut windows, &mut winit_windows);

                    let primary_window = winit_windows
                        .get_window(WindowId::primary())
                        .expect("no primary window");

                    primary_window.request_redraw();

                    // Run window commands
                    for window in windows.iter_mut() {
                        let winit_window =
                            if let Some(winit_window) = winit_windows.get_window(window.id()) {
                                winit_window
                            } else {
                                continue;
                            };

                        for command in window.drain_commands() {
                            match command {
                                WindowCommand::SetWindowMode { .. } => todo!(),
                                WindowCommand::SetTitle { title } => {
                                    winit_window.set_title(title.as_str())
                                }
                                WindowCommand::SetScaleFactor { .. } => todo!(),
                                WindowCommand::SetResolution { .. } => todo!(),
                                WindowCommand::SetVsync { .. } => todo!(),
                                WindowCommand::SetResizable { resizable } => {
                                    winit_window.set_resizable(resizable)
                                }
                                WindowCommand::SetDecorations { decorations } => {
                                    winit_window.set_decorations(decorations)
                                }
                                WindowCommand::SetCursorLockMode { locked } => winit_window
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
                                    winit_window.set_cursor_visible(visible)
                                }
                                WindowCommand::SetCursorPosition { position } => winit_window
                                    .set_cursor_position(Position::Logical(LogicalPosition::new(
                                        position.x as f64,
                                        position.y as f64,
                                    )))
                                    .unwrap(),
                                WindowCommand::SetMaximized { maximized } => {
                                    winit_window.set_maximized(maximized)
                                }
                                WindowCommand::SetMinimized { minimized } => {
                                    winit_window.set_minimized(minimized)
                                }
                                WindowCommand::SetPosition { .. } => todo!(),
                                WindowCommand::SetResizeConstraints { .. } => {
                                    todo!()
                                }
                            }
                        }
                    }

                    // Drop so systems in the dispatcher can access these
                    std::mem::drop(windows);
                    std::mem::drop(winit_windows);
                    std::mem::drop(input);

                    // Compute delta time and submit tick
                    let now = Instant::now();
                    dispatcher.submit(Tick(now.duration_since(last)));
                    last = now;

                    // Dispatch until events are cleared
                    dispatcher.run(&mut app.world, &app.resources);

                    // Reset input state
                    app.resources.get_mut::<InputState>().unwrap().flush();
                }
            }
            _ => {}
        }
    });
}

fn create_windows(
    event_loop: &EventLoopWindowTarget<()>,
    windows: &mut Windows,
    winit_windows: &mut WinitWindows,
) {
    let mut new_windows = Vec::default();
    for (descriptor, id) in windows.drain_to_create() {
        new_windows.push(winit_windows.create_window(event_loop, id, descriptor));
    }

    for window in new_windows {
        windows.add(window);
    }
}

#[inline]
fn winit_to_ard_key(key: winit::event::VirtualKeyCode) -> Option<Key> {
    let ard_key = match key {
        winit::event::VirtualKeyCode::Key1 => Key::Key1,
        winit::event::VirtualKeyCode::Key2 => Key::Key2,
        winit::event::VirtualKeyCode::Key3 => Key::Key3,
        winit::event::VirtualKeyCode::Key4 => Key::Key4,
        winit::event::VirtualKeyCode::Key5 => Key::Key5,
        winit::event::VirtualKeyCode::Key6 => Key::Key6,
        winit::event::VirtualKeyCode::Key7 => Key::Key7,
        winit::event::VirtualKeyCode::Key8 => Key::Key8,
        winit::event::VirtualKeyCode::Key9 => Key::Key9,
        winit::event::VirtualKeyCode::Key0 => Key::Key0,
        winit::event::VirtualKeyCode::A => Key::A,
        winit::event::VirtualKeyCode::B => Key::B,
        winit::event::VirtualKeyCode::C => Key::C,
        winit::event::VirtualKeyCode::D => Key::D,
        winit::event::VirtualKeyCode::E => Key::E,
        winit::event::VirtualKeyCode::F => Key::F,
        winit::event::VirtualKeyCode::G => Key::G,
        winit::event::VirtualKeyCode::H => Key::H,
        winit::event::VirtualKeyCode::I => Key::I,
        winit::event::VirtualKeyCode::J => Key::J,
        winit::event::VirtualKeyCode::K => Key::K,
        winit::event::VirtualKeyCode::L => Key::L,
        winit::event::VirtualKeyCode::M => Key::M,
        winit::event::VirtualKeyCode::N => Key::N,
        winit::event::VirtualKeyCode::O => Key::O,
        winit::event::VirtualKeyCode::P => Key::P,
        winit::event::VirtualKeyCode::Q => Key::Q,
        winit::event::VirtualKeyCode::R => Key::R,
        winit::event::VirtualKeyCode::S => Key::S,
        winit::event::VirtualKeyCode::T => Key::T,
        winit::event::VirtualKeyCode::U => Key::U,
        winit::event::VirtualKeyCode::V => Key::V,
        winit::event::VirtualKeyCode::W => Key::W,
        winit::event::VirtualKeyCode::X => Key::X,
        winit::event::VirtualKeyCode::Y => Key::Y,
        winit::event::VirtualKeyCode::Z => Key::Z,
        winit::event::VirtualKeyCode::Escape => Key::Escape,
        winit::event::VirtualKeyCode::F1 => Key::F1,
        winit::event::VirtualKeyCode::F2 => Key::F2,
        winit::event::VirtualKeyCode::F3 => Key::F3,
        winit::event::VirtualKeyCode::F4 => Key::F4,
        winit::event::VirtualKeyCode::F5 => Key::F5,
        winit::event::VirtualKeyCode::F6 => Key::F6,
        winit::event::VirtualKeyCode::F7 => Key::F7,
        winit::event::VirtualKeyCode::F8 => Key::F8,
        winit::event::VirtualKeyCode::F9 => Key::F9,
        winit::event::VirtualKeyCode::F10 => Key::F10,
        winit::event::VirtualKeyCode::F11 => Key::F11,
        winit::event::VirtualKeyCode::F12 => Key::F12,
        winit::event::VirtualKeyCode::F13 => Key::F13,
        winit::event::VirtualKeyCode::F14 => Key::F14,
        winit::event::VirtualKeyCode::F15 => Key::F15,
        winit::event::VirtualKeyCode::F16 => Key::F16,
        winit::event::VirtualKeyCode::F17 => Key::F17,
        winit::event::VirtualKeyCode::F18 => Key::F18,
        winit::event::VirtualKeyCode::F19 => Key::F19,
        winit::event::VirtualKeyCode::F20 => Key::F20,
        winit::event::VirtualKeyCode::F21 => Key::F21,
        winit::event::VirtualKeyCode::F22 => Key::F22,
        winit::event::VirtualKeyCode::F23 => Key::F23,
        winit::event::VirtualKeyCode::F24 => Key::F24,
        winit::event::VirtualKeyCode::Snapshot => Key::PrintScreen,
        winit::event::VirtualKeyCode::Scroll => Key::ScrollLock,
        winit::event::VirtualKeyCode::Pause => Key::Pause,
        winit::event::VirtualKeyCode::Insert => Key::Insert,
        winit::event::VirtualKeyCode::Home => Key::Home,
        winit::event::VirtualKeyCode::Delete => Key::Delete,
        winit::event::VirtualKeyCode::End => Key::End,
        winit::event::VirtualKeyCode::PageDown => Key::PageDown,
        winit::event::VirtualKeyCode::PageUp => Key::PageUp,
        winit::event::VirtualKeyCode::Left => Key::Left,
        winit::event::VirtualKeyCode::Up => Key::Up,
        winit::event::VirtualKeyCode::Right => Key::Right,
        winit::event::VirtualKeyCode::Down => Key::Down,
        winit::event::VirtualKeyCode::Back => Key::Back,
        winit::event::VirtualKeyCode::Return => Key::Return,
        winit::event::VirtualKeyCode::Space => Key::Space,
        winit::event::VirtualKeyCode::Numlock => Key::Numlock,
        winit::event::VirtualKeyCode::Numpad0 => Key::Numpad0,
        winit::event::VirtualKeyCode::Numpad1 => Key::Numpad1,
        winit::event::VirtualKeyCode::Numpad2 => Key::Numpad2,
        winit::event::VirtualKeyCode::Numpad3 => Key::Numpad3,
        winit::event::VirtualKeyCode::Numpad4 => Key::Numpad4,
        winit::event::VirtualKeyCode::Numpad5 => Key::Numpad5,
        winit::event::VirtualKeyCode::Numpad6 => Key::Numpad6,
        winit::event::VirtualKeyCode::Numpad7 => Key::Numpad7,
        winit::event::VirtualKeyCode::Numpad8 => Key::Numpad8,
        winit::event::VirtualKeyCode::Numpad9 => Key::Numpad9,
        winit::event::VirtualKeyCode::NumpadAdd => Key::NumAdd,
        winit::event::VirtualKeyCode::NumpadDivide => Key::NumDivide,
        winit::event::VirtualKeyCode::NumpadDecimal => Key::NumDecimal,
        winit::event::VirtualKeyCode::NumpadMultiply => Key::NumMultiply,
        winit::event::VirtualKeyCode::NumpadSubtract => Key::NumSubtract,
        winit::event::VirtualKeyCode::NumpadEnter => Key::NumEnter,
        winit::event::VirtualKeyCode::Apostrophe => Key::Apostrophe,
        winit::event::VirtualKeyCode::Backslash => Key::Backslash,
        winit::event::VirtualKeyCode::Comma => Key::Comma,
        winit::event::VirtualKeyCode::Equals => Key::Equals,
        winit::event::VirtualKeyCode::Grave => Key::Tilde,
        winit::event::VirtualKeyCode::LAlt => Key::LAlt,
        winit::event::VirtualKeyCode::LBracket => Key::LBracket,
        winit::event::VirtualKeyCode::LControl => Key::LCtrl,
        winit::event::VirtualKeyCode::LShift => Key::LShift,
        winit::event::VirtualKeyCode::LWin => Key::LWin,
        winit::event::VirtualKeyCode::Minus => Key::Minus,
        winit::event::VirtualKeyCode::Mute => Key::Mute,
        winit::event::VirtualKeyCode::Period => Key::Period,
        winit::event::VirtualKeyCode::RAlt => Key::RAlt,
        winit::event::VirtualKeyCode::RBracket => Key::RBracket,
        winit::event::VirtualKeyCode::RControl => Key::RCtrl,
        winit::event::VirtualKeyCode::RShift => Key::RShift,
        winit::event::VirtualKeyCode::RWin => Key::RWin,
        winit::event::VirtualKeyCode::Semicolon => Key::Semicolon,
        winit::event::VirtualKeyCode::Slash => Key::Slash,
        winit::event::VirtualKeyCode::Tab => Key::Tab,
        winit::event::VirtualKeyCode::VolumeDown => Key::VolumeDown,
        winit::event::VirtualKeyCode::VolumeUp => Key::VolumeUp,
        _ => return None,
    };

    Some(ard_key)
}

#[inline]
fn winit_to_ard_mouse_button(button: winit::event::MouseButton) -> Option<MouseButton> {
    let ard_button = match button {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        _ => return None,
    };

    Some(ard_button)
}
