use crate::app::AppBuilder;

pub trait Plugin {
    fn build(&mut self, app: &mut AppBuilder);

    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}
