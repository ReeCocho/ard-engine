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

pub mod game {
    pub use ard_game::*;
}

pub mod assets {
    pub use ard_assets::*;
}

pub mod input {
    pub use ard_input::*;
}

pub mod window {
    pub mod prelude {
        pub use ard_window::prelude::*;
        pub use ard_winit::prelude::*;
    }
}

pub mod render {
    pub mod prelude {
        pub use ard_pal::prelude::*;
    }

    pub use ard_pal::*;
    pub use ard_render::*;
    pub use ard_render_assets::*;
    pub use ard_render_camera::*;
    pub use ard_render_gui::*;
    pub use ard_render_objects::*;
}
