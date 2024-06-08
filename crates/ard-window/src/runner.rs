use std::{cmp::Ordering, time::Instant};

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_input::{InputState, Key, MouseButton};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    keyboard::PhysicalKey,
};

use crate::{
    prelude::WindowId, windows::Windows, ExitOnClose, WindowClosed, WindowFileDropped,
    WindowPlugin, WindowResized,
};

struct WinitApp {
    resources: Resources,
    world: World,
    dispatcher: Dispatcher,
    last: Instant,
}

impl ApplicationHandler for WinitApp {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let mut windows = self.resources.get_mut::<Windows>().unwrap();
        windows.add_pending(event_loop);
        for window in windows.iter_mut() {
            window.apply_commands();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        let mut input = self.resources.get_mut::<InputState>().unwrap();

        match event {
            winit::event::DeviceEvent::Key(key) => {
                let code = match key.physical_key {
                    PhysicalKey::Code(code) => code,
                    _ => return,
                };

                let ard_key = match winit_to_ard_key(code) {
                    Some(key) => key,
                    None => return,
                };

                match key.state {
                    winit::event::ElementState::Pressed => {
                        input.signal_key_down(ard_key);
                    }
                    winit::event::ElementState::Released => {
                        input.signal_key_up(ard_key);
                    }
                }
            }
            winit::event::DeviceEvent::MouseMotion { delta } => {
                input.signal_mouse_movement(delta);
            }
            _ => {}
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let mut windows = self.resources.get_mut::<Windows>().unwrap();
        let mut input = self.resources.get_mut::<InputState>().unwrap();

        // Get the Ard ID of the window
        let ard_id = windows.winit_to_ard_id(window_id).unwrap();
        let window = windows.get_mut(ard_id).unwrap();

        // Keep requesting redraw
        window.winit_window.request_redraw();

        match event {
            WindowEvent::Resized(dims) => {
                self.dispatcher.submit(WindowResized {
                    id: ard_id,
                    width: dims.width,
                    height: dims.height,
                });
                window.update_actual_size_from_backend(dims.width, dims.height);
            }
            WindowEvent::CloseRequested => {
                self.dispatcher.submit(WindowClosed(ard_id));
            }
            WindowEvent::DroppedFile(path) => {
                self.dispatcher.event_sender().submit(WindowFileDropped {
                    window: ard_id,
                    file: path,
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(text) = event.text {
                    for ch in text.chars() {
                        if ch != '\u{7f}' {
                            input.signal_character(ch);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                input.signal_mouse_pos((position.x, position.y));
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
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
            WindowEvent::RedrawRequested => {
                // Check if `Stop` was requested
                if self.resources.get::<ArdCoreState>().unwrap().stopping() {
                    event_loop.exit();

                    // Handle `Stopping` event
                    self.dispatcher.run(&mut self.world, &self.resources);
                } else {
                    // Create windows if needed and request redraw from the primary window
                    windows.add_pending(event_loop);

                    // Run window commands
                    for window in windows.iter_mut() {
                        window.apply_commands();
                    }

                    // Drop so systems in the dispatcher can access these
                    std::mem::drop(windows);
                    std::mem::drop(input);

                    // Compute delta time and submit tick
                    let now = Instant::now();
                    self.dispatcher.submit(Tick(now.duration_since(self.last)));
                    self.last = now;

                    // Dispatch until events are cleared
                    self.dispatcher.run(&mut self.world, &self.resources);

                    // Reset input state
                    self.resources.get_mut::<InputState>().unwrap().flush();
                }
            }
            _ => {}
        }
    }
}

pub fn winit_runner(mut app: App) {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut windows = Windows::new(event_loop.owned_display_handle());
    let mut plugin = app.resources.get::<WindowPlugin>().unwrap().clone();

    if let Some(descriptor) = std::mem::take(&mut plugin.add_primary_window) {
        windows.create(WindowId::primary(), descriptor);
    }

    if plugin.exit_on_close {
        app.dispatcher.add_system(ExitOnClose);
    }

    app.resources.add(windows);

    app.run_startups();
    let mut winit_app = WinitApp {
        resources: app.resources,
        world: app.world,
        dispatcher: app.dispatcher.build(),
        last: Instant::now(),
    };

    event_loop.run_app(&mut winit_app).unwrap();
}

#[inline]
fn winit_to_ard_key(key: winit::keyboard::KeyCode) -> Option<Key> {
    let ard_key = match key {
        winit::keyboard::KeyCode::Digit1 => Key::Key1,
        winit::keyboard::KeyCode::Digit2 => Key::Key2,
        winit::keyboard::KeyCode::Digit3 => Key::Key3,
        winit::keyboard::KeyCode::Digit4 => Key::Key4,
        winit::keyboard::KeyCode::Digit5 => Key::Key5,
        winit::keyboard::KeyCode::Digit6 => Key::Key6,
        winit::keyboard::KeyCode::Digit7 => Key::Key7,
        winit::keyboard::KeyCode::Digit8 => Key::Key8,
        winit::keyboard::KeyCode::Digit9 => Key::Key9,
        winit::keyboard::KeyCode::Digit0 => Key::Key0,
        winit::keyboard::KeyCode::KeyA => Key::A,
        winit::keyboard::KeyCode::KeyB => Key::B,
        winit::keyboard::KeyCode::KeyC => Key::C,
        winit::keyboard::KeyCode::KeyD => Key::D,
        winit::keyboard::KeyCode::KeyE => Key::E,
        winit::keyboard::KeyCode::KeyF => Key::F,
        winit::keyboard::KeyCode::KeyG => Key::G,
        winit::keyboard::KeyCode::KeyH => Key::H,
        winit::keyboard::KeyCode::KeyI => Key::I,
        winit::keyboard::KeyCode::KeyJ => Key::J,
        winit::keyboard::KeyCode::KeyK => Key::K,
        winit::keyboard::KeyCode::KeyL => Key::L,
        winit::keyboard::KeyCode::KeyM => Key::M,
        winit::keyboard::KeyCode::KeyN => Key::N,
        winit::keyboard::KeyCode::KeyO => Key::O,
        winit::keyboard::KeyCode::KeyP => Key::P,
        winit::keyboard::KeyCode::KeyQ => Key::Q,
        winit::keyboard::KeyCode::KeyR => Key::R,
        winit::keyboard::KeyCode::KeyS => Key::S,
        winit::keyboard::KeyCode::KeyT => Key::T,
        winit::keyboard::KeyCode::KeyU => Key::U,
        winit::keyboard::KeyCode::KeyV => Key::V,
        winit::keyboard::KeyCode::KeyW => Key::W,
        winit::keyboard::KeyCode::KeyX => Key::X,
        winit::keyboard::KeyCode::KeyY => Key::Y,
        winit::keyboard::KeyCode::KeyZ => Key::Z,
        winit::keyboard::KeyCode::Escape => Key::Escape,
        winit::keyboard::KeyCode::F1 => Key::F1,
        winit::keyboard::KeyCode::F2 => Key::F2,
        winit::keyboard::KeyCode::F3 => Key::F3,
        winit::keyboard::KeyCode::F4 => Key::F4,
        winit::keyboard::KeyCode::F5 => Key::F5,
        winit::keyboard::KeyCode::F6 => Key::F6,
        winit::keyboard::KeyCode::F7 => Key::F7,
        winit::keyboard::KeyCode::F8 => Key::F8,
        winit::keyboard::KeyCode::F9 => Key::F9,
        winit::keyboard::KeyCode::F10 => Key::F10,
        winit::keyboard::KeyCode::F11 => Key::F11,
        winit::keyboard::KeyCode::F12 => Key::F12,
        winit::keyboard::KeyCode::F13 => Key::F13,
        winit::keyboard::KeyCode::F14 => Key::F14,
        winit::keyboard::KeyCode::F15 => Key::F15,
        winit::keyboard::KeyCode::F16 => Key::F16,
        winit::keyboard::KeyCode::F17 => Key::F17,
        winit::keyboard::KeyCode::F18 => Key::F18,
        winit::keyboard::KeyCode::F19 => Key::F19,
        winit::keyboard::KeyCode::F20 => Key::F20,
        winit::keyboard::KeyCode::F21 => Key::F21,
        winit::keyboard::KeyCode::F22 => Key::F22,
        winit::keyboard::KeyCode::F23 => Key::F23,
        winit::keyboard::KeyCode::F24 => Key::F24,
        winit::keyboard::KeyCode::PrintScreen => Key::PrintScreen,
        winit::keyboard::KeyCode::ScrollLock => Key::ScrollLock,
        winit::keyboard::KeyCode::Pause => Key::Pause,
        winit::keyboard::KeyCode::Insert => Key::Insert,
        winit::keyboard::KeyCode::Home => Key::Home,
        winit::keyboard::KeyCode::Delete => Key::Delete,
        winit::keyboard::KeyCode::End => Key::End,
        winit::keyboard::KeyCode::PageDown => Key::PageDown,
        winit::keyboard::KeyCode::PageUp => Key::PageUp,
        winit::keyboard::KeyCode::ArrowLeft => Key::Left,
        winit::keyboard::KeyCode::ArrowUp => Key::Up,
        winit::keyboard::KeyCode::ArrowRight => Key::Right,
        winit::keyboard::KeyCode::ArrowDown => Key::Down,
        winit::keyboard::KeyCode::Backspace => Key::Back,
        winit::keyboard::KeyCode::Enter => Key::Return,
        winit::keyboard::KeyCode::Space => Key::Space,
        winit::keyboard::KeyCode::NumLock => Key::Numlock,
        winit::keyboard::KeyCode::Numpad0 => Key::Numpad0,
        winit::keyboard::KeyCode::Numpad1 => Key::Numpad1,
        winit::keyboard::KeyCode::Numpad2 => Key::Numpad2,
        winit::keyboard::KeyCode::Numpad3 => Key::Numpad3,
        winit::keyboard::KeyCode::Numpad4 => Key::Numpad4,
        winit::keyboard::KeyCode::Numpad5 => Key::Numpad5,
        winit::keyboard::KeyCode::Numpad6 => Key::Numpad6,
        winit::keyboard::KeyCode::Numpad7 => Key::Numpad7,
        winit::keyboard::KeyCode::Numpad8 => Key::Numpad8,
        winit::keyboard::KeyCode::Numpad9 => Key::Numpad9,
        winit::keyboard::KeyCode::NumpadAdd => Key::NumAdd,
        winit::keyboard::KeyCode::NumpadDivide => Key::NumDivide,
        winit::keyboard::KeyCode::NumpadDecimal => Key::NumDecimal,
        winit::keyboard::KeyCode::NumpadMultiply => Key::NumMultiply,
        winit::keyboard::KeyCode::NumpadSubtract => Key::NumSubtract,
        winit::keyboard::KeyCode::NumpadEnter => Key::NumEnter,
        winit::keyboard::KeyCode::Backslash => Key::Backslash,
        winit::keyboard::KeyCode::Comma => Key::Comma,
        winit::keyboard::KeyCode::Equal => Key::Equals,
        winit::keyboard::KeyCode::Backquote => Key::Tilde,
        winit::keyboard::KeyCode::AltLeft => Key::LAlt,
        winit::keyboard::KeyCode::BracketLeft => Key::LBracket,
        winit::keyboard::KeyCode::ControlLeft => Key::LCtrl,
        winit::keyboard::KeyCode::ShiftLeft => Key::LShift,
        winit::keyboard::KeyCode::SuperLeft => Key::LWin,
        winit::keyboard::KeyCode::Minus => Key::Minus,
        winit::keyboard::KeyCode::AudioVolumeMute => Key::Mute,
        winit::keyboard::KeyCode::Period => Key::Period,
        winit::keyboard::KeyCode::AltRight => Key::RAlt,
        winit::keyboard::KeyCode::BracketRight => Key::RBracket,
        winit::keyboard::KeyCode::ControlRight => Key::RCtrl,
        winit::keyboard::KeyCode::ShiftRight => Key::RShift,
        winit::keyboard::KeyCode::SuperRight => Key::RWin,
        winit::keyboard::KeyCode::Semicolon => Key::Semicolon,
        winit::keyboard::KeyCode::Slash => Key::Slash,
        winit::keyboard::KeyCode::Tab => Key::Tab,
        winit::keyboard::KeyCode::AudioVolumeDown => Key::VolumeDown,
        winit::keyboard::KeyCode::AudioVolumeUp => Key::VolumeUp,
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
