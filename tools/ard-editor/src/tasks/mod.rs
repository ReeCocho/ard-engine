pub mod asset;
pub mod build;
pub mod instantiate;
pub mod load;
pub mod material;
pub mod model;
pub mod play;
pub mod save;
pub mod texture;

use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

use anyhow::Result;
use ard_engine::{core::prelude::*, ecs::prelude::*, log::*, render::view::GuiView};
use crossbeam_channel::{Receiver, Sender};

pub trait EditorTask: Send {
    fn has_confirm_ui(&self) -> bool {
        true
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation>;

    fn state(&mut self) -> Option<TaskState> {
        None
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) -> Result<()> {
        Ok(())
    }

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
    /// Receives task states after they've been confirmed.
    recv_state: Receiver<TaskState>,
}

pub struct TaskConfirmationGui {
    /// Receives tasks from the queue.
    task_recv: Receiver<Box<dyn EditorTask>>,
    err_recv: Receiver<anyhow::Error>,
    /// Sends tasks to the runner after confirmation.
    send: Sender<Box<dyn EditorTask>>,
    /// Sends task state to the queue after confirmation.
    send_state: Sender<TaskState>,
    pending: Option<PendingTask>,
    errors: Vec<anyhow::Error>,
}

#[derive(SystemState)]
pub struct TaskRunner {
    /// Receives tasks from the confirmation GUI.
    recv: Receiver<Box<dyn EditorTask>>,
    err_send: Sender<anyhow::Error>,
    running: Option<JoinHandle<Result<Box<dyn EditorTask>>>>,
}

#[derive(Clone)]
pub struct TaskState {
    state: Arc<AtomicU32>,
    name: String,
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

    #[inline(always)]
    pub fn recv_state(&self) -> Option<TaskState> {
        self.recv_state.try_recv().ok()
    }
}

impl GuiView for TaskConfirmationGui {
    fn show(
        &mut self,
        _tick: Tick,
        ctx: &egui::Context,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) {
        // Receive errors
        while let Ok(new_err) = self.err_recv.try_recv() {
            self.errors.push(new_err);
        }

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
                    if let Some(state) = pending.task.state() {
                        if let Err(err) = self.send_state.send(state) {
                            warn!("error sending task state: {:?}", err);
                        }
                    }

                    if let Err(err) = self.send.send(pending.task) {
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

        if let Some(err) = self.errors.last() {
            let mut pop_err = false;
            let err_idx = self.errors.len();
            egui::Window::new(format!("Error ({err_idx})"))
                .collapsible(false)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.screen_rect().center())
                .show(ctx, |ui| {
                    ui.label(err.to_string());
                    if ui.button("Close").clicked() {
                        pop_err = true;
                    }
                });
            if pop_err {
                self.errors.pop();
            }
        }
    }
}

impl PendingTask {
    fn show(&mut self, ctx: &egui::Context) -> TaskConfirmation {
        // Early out if we don't have confirm UI
        if !self.task.has_confirm_ui() {
            return TaskConfirmation::Ready;
        }

        let mut out = TaskConfirmation::Wait;

        egui::Window::new("Confirmation")
            .collapsible(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.screen_rect().center())
            .show(ctx, |ui| {
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
        let (send_state, recv_state) = crossbeam_channel::unbounded();
        let (gui_send, runner_recv) = crossbeam_channel::unbounded();
        let (err_send, err_recv) = crossbeam_channel::unbounded();

        let queue = TaskQueue {
            send: queue_send,
            recv_state,
        };

        let gui = TaskConfirmationGui {
            task_recv: gui_recv,
            err_recv,
            send: gui_send,
            send_state,
            pending: None,
            errors: Vec::default(),
        };

        let runner = TaskRunner {
            recv: runner_recv,
            err_send,
            running: None,
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
        // Check the current task
        if let Some(handle) = self.running.take() {
            if !handle.is_finished() {
                self.running = Some(handle);
                return;
            }

            let thread_result = handle.join();

            let result = match thread_result {
                Ok(result) => result,
                Err(err) => {
                    let err = anyhow::Error::msg(
                        err.downcast::<String>().map(|s| *s).unwrap_or_else(|s| {
                            s.downcast::<&'static str>()
                                .map_or_else(|_| "unknown error".into(), |s| s.to_string())
                        }),
                    );
                    let _ = self.err_send.send(err);
                    return;
                }
            };

            let mut task = match result {
                Ok(task) => task,
                Err(err) => {
                    let _ = self.err_send.send(err);
                    return;
                }
            };

            if let Err(err) = task.complete(&commands, &queries, &res) {
                let _ = self.err_send.send(err);
            }
        } else if let Ok(mut task) = self.recv.try_recv() {
            if let Err(err) = task.pre_run(&commands, &queries, &res) {
                if let Some(state) = task.state() {
                    state.fail();
                }
                let _ = self.err_send.send(err);
                return;
            }

            self.running = Some(std::thread::spawn(move || TaskRunner::run_task(task)));
        }
    }

    fn run_task(mut task: Box<dyn EditorTask>) -> Result<Box<dyn EditorTask>> {
        match task.run() {
            Ok(_) => {
                if let Some(state) = task.state() {
                    state.success();
                }
                Ok(task)
            }
            Err(err) => {
                if let Some(state) = task.state() {
                    state.fail();
                }
                Err(err)
            }
        }
    }
}

impl TaskState {
    const NORMALIZED_RANGE: u32 = 1024;

    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: Arc::new(AtomicU32::new(0)),
        }
    }

    #[inline(always)]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_completion(&self, completion: f32) {
        let completion = completion.clamp(0.0, 1.0);
        let normalized = (completion * Self::NORMALIZED_RANGE as f32) as u32;
        self.state.store(normalized, Ordering::Relaxed);
    }

    pub fn completion(&self) -> f32 {
        let mut v = self.state.load(Ordering::Relaxed);
        v &= (1 << 31) - 1;
        (v as f32 / Self::NORMALIZED_RANGE as f32).clamp(0.0, 1.0)
    }

    pub fn fail(&self) {
        let mut val = 1 << 31;
        val |= Self::NORMALIZED_RANGE;
        self.state.store(val, Ordering::Relaxed);
    }

    pub fn success(&self) {
        self.state.store(Self::NORMALIZED_RANGE, Ordering::Relaxed);
    }

    pub fn succeeded(&self) -> Option<bool> {
        let v = self.state.load(Ordering::Relaxed);
        if v & ((1 << 31) - 1) != Self::NORMALIZED_RANGE {
            return None;
        }
        Some(v & (1 << 31) == 0)
    }
}

impl From<TaskRunner> for System {
    fn from(value: TaskRunner) -> Self {
        SystemBuilder::new(value)
            .with_handler(TaskRunner::on_tick)
            .build()
    }
}
