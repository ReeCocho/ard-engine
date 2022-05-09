use ard_ecs::prelude::*;

/// Interface for user input. Populated by a backend like `Winit`.
#[derive(Debug, Resource)]
pub struct InputState {
    mouse_delta: (f64, f64),
    key_state: [KeyState; u8::MAX as usize],
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
    LBracket,
    RBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Comma,
    Period,
    Slash,
}

#[derive(Debug, Default, Copy, Clone)]
struct KeyState {
    down: bool,
    down_repeat: bool,
    held: bool,
    up: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            mouse_delta: (0.0, 0.0),
            key_state: [KeyState::default(); u8::MAX as usize],
        }
    }
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Relative position of the mouse from last frame.
    #[inline]
    pub fn mouse_delta(&self) -> (f64, f64) {
        self.mouse_delta
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

    /// Signal that the mouse has moved.
    #[inline]
    pub fn signal_mouse_movement(&mut self, delta: (f64, f64)) {
        self.mouse_delta.0 += delta.0;
        self.mouse_delta.1 += delta.1;
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

    /// Indicates that input state should be reset for the next tick.
    #[inline]
    pub fn flush(&mut self) {
        self.mouse_delta = (0.0, 0.0);

        for key in &mut self.key_state {
            key.down = false;
            key.down_repeat = false;
            key.up = false;
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<u8> for Key {
    fn into(self) -> u8 {
        unsafe { std::mem::transmute(self) }
    }
}
