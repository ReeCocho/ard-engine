use std::{collections::VecDeque, thread::JoinHandle};

use anyhow::Result;
use ard_engine::{core::prelude::*, ecs::prelude::*, log::*, render::view::GuiView};
use crossbeam_channel::{Receiver, Sender};

pub trait EditorTask: Send {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation>;

    fn run(&mut self) -> Result<()>;

    fn complete(
        &mut self,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()>;
}

pub enum TaskConfirmation {
    Ready,
    Cancel,
    Wait,
}

#[derive(Resource)]
pub struct TaskQueue {
    /// Sends new tasks to the confirmation GUI.
    send: Sender<Box<dyn EditorTask>>,
}

pub struct TaskConfirmationGui {
    /// Receives tasks from the queue.
    task_recv: Receiver<Box<dyn EditorTask>>,
    err_recv: Receiver<anyhow::Error>,
    /// Sends tasks to the runner after confirmation.
    send: Sender<JoinHandle<Result<Box<dyn EditorTask>>>>,
    pending: Option<PendingTask>,
}

#[derive(SystemState)]
pub struct TaskRunner {
    /// Receives tasks from the confirmation GUI.
    recv: Receiver<JoinHandle<Result<Box<dyn EditorTask>>>>,
    err_send: Sender<anyhow::Error>,
    running: VecDeque<JoinHandle<Result<Box<dyn EditorTask>>>>,
}

struct PendingTask {
    task: Box<dyn EditorTask>,
}

impl TaskQueue {
    #[inline(always)]
    pub fn add(&self, task: impl EditorTask + 'static) {
        if let Err(err) = self.send.send(Box::new(task)) {
            warn!("error adding task: {:?}", err);
        }
    }
}

impl GuiView for TaskConfirmationGui {
    fn show(
        &mut self,
        tick: Tick,
        ctx: &egui::Context,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        // Handle new tasks and process existing ones
        match self.pending.take() {
            Some(mut pending) => match pending.show(ctx) {
                TaskConfirmation::Wait => {
                    self.pending = Some(pending);
                }
                TaskConfirmation::Cancel => {
                    self.pending = None;
                }
                TaskConfirmation::Ready => {
                    if let Err(err) = self.send.send(std::thread::spawn(move || {
                        TaskRunner::run_task(pending.task)
                    })) {
                        warn!("error spawning task: {:?}", err);
                    }
                }
            },
            None => {
                if let Ok(task) = self.task_recv.try_recv() {
                    self.pending = Some(PendingTask { task });
                }
            }
        }
    }
}

impl PendingTask {
    fn show(&mut self, ctx: &egui::Context) -> TaskConfirmation {
        let mut out = TaskConfirmation::Wait;

        egui::Window::new("Confirmation").show(ctx, |ui| {
            match self.task.confirm_ui(ui) {
                Ok(res) => out = res,
                Err(_) => {
                    // TODO: Deal with error messages
                    out = TaskConfirmation::Cancel;
                }
            }
        });

        out
    }
}

impl TaskRunner {
    pub fn new() -> (Self, TaskConfirmationGui, TaskQueue) {
        let (queue_send, gui_recv) = crossbeam_channel::unbounded();
        let (gui_send, runner_recv) = crossbeam_channel::unbounded();
        let (err_send, err_recv) = crossbeam_channel::unbounded();

        let queue = TaskQueue { send: queue_send };

        let gui = TaskConfirmationGui {
            task_recv: gui_recv,
            err_recv,
            send: gui_send,
            pending: None,
        };

        let runner = TaskRunner {
            recv: runner_recv,
            err_send,
            running: VecDeque::default(),
        };

        (runner, gui, queue)
    }

    fn on_tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<Everything>,
        res: Res<Everything>,
    ) {
        // Retrieve new tasks
        while let Ok(task) = self.recv.try_recv() {
            self.running.push_back(task);
        }

        // Check if any tasks have finished
        let mut needs_check = self.running.len();
        while needs_check != 0 {
            let handle = self.running.pop_front().unwrap();
            needs_check -= 1;

            if !handle.is_finished() {
                self.running.push_back(handle);
                continue;
            }

            let thread_result = handle.join();

            // TODO: Put errors onto GUI
            let result = match thread_result {
                Ok(result) => result,
                Err(err) => {
                    error!("{err:?}");
                    continue;
                }
            };

            let mut task = match result {
                Ok(task) => task,
                Err(err) => {
                    error!("{err:?}");
                    continue;
                }
            };

            if let Err(err) = task.complete(&commands, &queries, &res) {
                error!("{err:?}");
            }
        }
    }

    fn run_task(mut task: Box<dyn EditorTask>) -> Result<Box<dyn EditorTask>> {
        match task.run() {
            Ok(_) => Ok(task),
            Err(err) => Err(err),
        }
    }
}

impl From<TaskRunner> for System {
    fn from(value: TaskRunner) -> Self {
        SystemBuilder::new(value)
            .with_handler(TaskRunner::on_tick)
            .build()
    }
}
