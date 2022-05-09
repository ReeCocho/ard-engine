pub mod core {
    pub use ard_core::*;
}

pub mod ecs {
    pub use ard_ecs::*;
}

pub mod log {
    pub use ard_log::*;
}

pub mod math {
    pub use ard_math::*;
}

#[cfg(feature = "assets")]
pub mod assets {
    pub use ard_assets::*;
}

#[cfg(feature = "graphics_vk")]
pub mod graphics {
    pub mod prelude {
        pub use ard_graphics_api::prelude::*;
        pub use ard_graphics_vk::prelude::*;
    }

    pub use ard_graphics_api::*;
    pub use ard_graphics_vk::*;
}

#[cfg(feature = "input")]
pub mod input {
    pub use ard_input::*;
}

#[cfg(feature = "window")]
pub mod window {
    pub mod prelude {
        pub use ard_window::prelude::*;
        pub use ard_winit::prelude::*;
    }

    pub use ard_window::*;
    pub use ard_winit::*;
}
