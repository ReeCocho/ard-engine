pub mod entity;
pub mod instantiate;

use std::collections::VecDeque;

use ard_engine::{
    core::core::Tick,
    ecs::prelude::*,
    input::{InputState, Key},
};

#[derive(Resource, Default)]
pub struct EditorCommands {
    pending: VecDeque<Box<dyn EditorCommand>>,
}

#[derive(SystemState)]
pub struct EditorCommandSystem {
    stack: Vec<Box<dyn EditorCommand>>,
    undone_stack: Vec<Box<dyn EditorCommand>>,
}

pub trait EditorCommand: Send + Sync + 'static {
    fn apply(&mut self, commands: &Commands, queries: &Queries<Everything>, res: &Res<Everything>);

    fn redo(&mut self, commands: &Commands, queries: &Queries<Everything>, res: &Res<Everything>) {
        self.apply(commands, queries, res);
    }

    fn undo(&mut self, commands: &Commands, queries: &Queries<Everything>, res: &Res<Everything>);

    fn clear(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) {
    }
}

impl EditorCommands {
    #[inline(always)]
    pub fn submit(&mut self, command: impl EditorCommand) {
        self.pending.push_back(Box::new(command));
    }
}

impl Default for EditorCommandSystem {
    fn default() -> Self {
        Self {
            stack: Vec::default(),
            undone_stack: Vec::default(),
        }
    }
}

impl EditorCommandSystem {
    pub fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<Everything>,
        res: Res<Everything>,
    ) {
        let mut editor_commands = res.get_mut::<EditorCommands>().unwrap();

        // If there are new commands, clear the undone stack
        if !editor_commands.pending.is_empty() {
            self.undone_stack.drain(..).for_each(|mut command| {
                command.clear(&commands, &queries, &res);
            })
        }

        // Apply new commands
        self.stack
            .extend(editor_commands.pending.drain(..).map(|mut command| {
                command.apply(&commands, &queries, &res);
                command
            }));

        let input = res.get::<InputState>().unwrap();
        let undo = input.key(Key::LCtrl) && input.key_down_repeat(Key::Z);
        let redo = input.key(Key::LCtrl) && input.key_down_repeat(Key::R);

        if undo {
            if let Some(mut command) = self.stack.pop() {
                command.undo(&commands, &queries, &res);
                self.undone_stack.push(command);
            }
        }

        if redo {
            if let Some(mut command) = self.undone_stack.pop() {
                command.redo(&commands, &queries, &res);
                self.stack.push(command);
            }
        }
    }
}

impl From<EditorCommandSystem> for System {
    fn from(value: EditorCommandSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(EditorCommandSystem::tick)
            .build()
    }
}
