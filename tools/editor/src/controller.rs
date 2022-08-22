use crate::editor::Resources;

#[derive(Default)]
pub struct Controller {
    /// Commands pending resolution
    commands: Vec<Box<dyn Command>>,
    /// List of resolved commands for undo operations.
    undo_stack: Vec<Box<dyn Command>>,
    /// List of resolved commands for redo operatins.
    redo_stack: Vec<Box<dyn Command>>,
}

pub trait Command: Send + 'static {
    #[inline]
    fn drain(self: Box<Self>, resc: &mut Resources) {}

    fn undo(&mut self, resc: &mut Resources);

    fn redo(&mut self, resc: &mut Resources);

    #[inline]
    fn apply(&mut self, resc: &mut Resources) {
        self.redo(resc);
    }
}

impl Controller {
    #[inline]
    pub fn submit(&mut self, command: impl Command) {
        self.commands.push(Box::new(command));
    }

    pub fn resolve(&mut self, resc: &mut Resources) {
        if self.commands.is_empty() {
            return;
        }

        // Since we have commands, we need to drain the redo stack because it is now invalid
        for command in self.redo_stack.drain(..) {
            command.drain(resc);
        }

        // Resolve commands
        for mut command in self.commands.drain(..) {
            command.apply(resc);
            self.undo_stack.push(command);
        }
    }

    pub fn undo(&mut self, resc: &mut Resources) {
        // If we are at the bottom of the stack, we cannot undo
        if self.undo_stack.is_empty() {
            return;
        }

        let mut command = self.undo_stack.pop().unwrap();
        command.undo(resc);
        self.redo_stack.push(command);
    }

    pub fn redo(&mut self, resc: &mut Resources) {
        // If we are at the top of the stack, we cannot redo
        if self.redo_stack.is_empty() {
            return;
        }

        let mut command = self.redo_stack.pop().unwrap();
        command.redo(resc);
        self.undo_stack.push(command);
    }
}
