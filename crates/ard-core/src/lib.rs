pub mod app;
pub mod core;
pub mod plugin;
#[cfg(test)]
mod tests;

pub mod prelude {
    pub use crate::app::*;
    pub use crate::core::*;
    pub use crate::plugin::*;
}
