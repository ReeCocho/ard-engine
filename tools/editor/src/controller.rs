#[derive(Default)]
pub struct Controller {
    /// Commands pending resolution
    commands: Vec<Command>,
    /// List of resolved commands for undo/redo operations.
    undo_stack: Vec<Command>,
    /// Index into `undo_stack` for the next command to undo. This value +1 points to the next
    /// command to redo.
    undo_stack_ptr: usize,
}

pub enum Command {}

impl Controller {
    #[inline]
    pub fn submit(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn resolve(&mut self) {
        if self.commands.is_empty() {
            return;
        }

        // Remove elements from the undo stack if we are pointing to the middle of the stack
        if !self.undo_stack.is_empty() && self.undo_stack.len() > self.undo_stack_ptr + 1 {
            self.undo_stack.truncate(self.undo_stack_ptr + 1);
        }

        // TODO: Resolve commands
    }
}
