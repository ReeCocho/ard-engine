use ard_ecs::prelude::*;

/// Interface for user input. Populated by a backend like `Winit`.
#[derive(Debug, Resource)]
pub struct InputState {
    mouse_delta: (f64, f64),
    mouse_position: (f64, f64),
    mouse_scroll: (f64, f64),
    key_state: [KeyState; u8::MAX as usize],
    mouse_state: [MouseState; u8::MAX as usize],
    input_string: String,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Space,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    PrintScreen,
    ScrollLock,
    Pause,
    Mute,
    VolumeDown,
    VolumeUp,
    Tilde,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    Minus,
    Equals,
    Back,
    Insert,
    Home,
    Delete,
    End,
    PageUp,
    PageDown,
    Tab,
    CapsLock,
    LShift,
    LCtrl,
    LWin,
    LAlt,
    Return,
    RShift,
    RCtrl,
    RWin,
    RAlt,
    Up,
    Down,
    Left,
    Right,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    Numpad0,
    Numlock,
    NumDivide,
    NumMultiply,
    NumSubtract,
    NumAdd,
    NumDecimal,
    NumEnter,
    LBracket,
    RBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Comma,
    Period,
    Slash,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Default, Copy, Clone)]
struct KeyState {
    down: bool,
    down_repeat: bool,
    held: bool,
    up: bool,
}

#[derive(Debug, Default, Copy, Clone)]
struct MouseState {
    down: bool,
    held: bool,
    up: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            mouse_delta: (0.0, 0.0),
            mouse_position: (0.0, 0.0),
            mouse_scroll: (0.0, 0.0),
            key_state: [KeyState::default(); u8::MAX as usize],
            mouse_state: [MouseState::default(); u8::MAX as usize],
            input_string: String::default(),
        }
    }
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn input_string(&self) -> &str {
        &self.input_string
    }

    /// Current mouse position in screen coordinates.
    #[inline]
    pub fn mouse_pos(&self) -> (f64, f64) {
        self.mouse_position
    }

    #[inline]
    pub fn mouse_scroll(&self) -> (f64, f64) {
        self.mouse_scroll
    }

    /// Relative position of the mouse from last frame.
    #[inline]
    pub fn mouse_delta(&self) -> (f64, f64) {
        self.mouse_delta
    }

    #[inline]
    pub fn mouse_button_down(&self, button: MouseButton) -> bool {
        self.mouse_state[button as usize].down
    }

    #[inline]
    pub fn mouse_button_up(&self, button: MouseButton) -> bool {
        self.mouse_state[button as usize].up
    }

    #[inline]
    pub fn mouse_button(&self, button: MouseButton) -> bool {
        self.mouse_state[button as usize].held
    }

    /// Returns `true` if the key was just pressed.
    #[inline]
    pub fn key_down(&self, key: Key) -> bool {
        self.key_state[key as usize].down
    }

    /// Returns `true` if the key was just pressed. Also accepts repeating pressed from holding
    /// down a key.
    #[inline]
    pub fn key_down_repeat(&self, key: Key) -> bool {
        self.key_state[key as usize].down_repeat
    }

    /// Returns `true` if the key was just released.
    #[inline]
    pub fn key_up(&self, key: Key) -> bool {
        self.key_state[key as usize].up
    }

    /// Returns `true` if the key is being held down.
    #[inline]
    pub fn key(&self, key: Key) -> bool {
        self.key_state[key as usize].held
    }

    /// Signal the moust position.
    #[inline]
    pub fn signal_mouse_pos(&mut self, pos: (f64, f64)) {
        self.mouse_position = pos;
    }

    /// Signal that the mouse has moved.
    #[inline]
    pub fn signal_mouse_movement(&mut self, delta: (f64, f64)) {
        self.mouse_delta.0 += delta.0;
        self.mouse_delta.1 += delta.1;
    }

    #[inline]
    pub fn signal_scroll(&mut self, delta: (f64, f64)) {
        self.mouse_scroll.0 += delta.0;
        self.mouse_scroll.1 += delta.1;
    }

    /// Signal that a key was just pressed down.
    #[inline]
    pub fn signal_key_down(&mut self, key: Key) {
        self.key_state[key as usize].down_repeat = true;

        // If a signal is sent to press a key, but it is already being held, then we have a case of
        // a "hold repeat"
        if !self.key_state[key as usize].held {
            self.key_state[key as usize].down = true;
        }

        self.key_state[key as usize].held = true;
    }

    /// Signal that a key was just released.
    #[inline]
    pub fn signal_key_up(&mut self, key: Key) {
        self.key_state[key as usize].up = true;
        self.key_state[key as usize].held = false;
    }

    #[inline]
    pub fn signal_mouse_button_down(&mut self, button: MouseButton) {
        // If a signal is sent to press a key, but it is already being held, then we have a case of
        // a "hold repeat"
        if !self.mouse_state[button as usize].held {
            self.mouse_state[button as usize].down = true;
        }

        self.mouse_state[button as usize].held = true;
    }

    #[inline]
    pub fn signal_mouse_button_up(&mut self, button: MouseButton) {
        self.mouse_state[button as usize].up = true;
        self.mouse_state[button as usize].held = false;
    }

    #[inline]
    pub fn signal_character(&mut self, c: char) {
        self.input_string.push(c);
    }

    /// Indicates that input state should be reset for the next tick.
    #[inline]
    pub fn flush(&mut self) {
        self.mouse_delta = (0.0, 0.0);
        self.mouse_scroll = (0.0, 0.0);
        self.input_string.clear();

        for key in &mut self.key_state {
            key.down = false;
            key.down_repeat = false;
            key.up = false;
        }

        for button in &mut self.mouse_state {
            button.down = false;
            button.up = false;
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<u8> for Key {
    fn into(self) -> u8 {
        unsafe { std::mem::transmute(self) }
    }
}

#[allow(clippy::from_over_into)]
impl Into<u8> for MouseButton {
    fn into(self) -> u8 {
        unsafe { std::mem::transmute(self) }
    }
}
