use std::collections::VecDeque;

use crate::util::par_task::{ParTask, ParTaskGet};
use thiserror::*;

#[derive(Default)]
pub struct EditorJobQueue {
    jobs: VecDeque<EditorJob>,
    active_job: Option<EditorJob>,
}

pub struct EditorJob {
    name: String,
    size: Option<(u32, u32)>,
    status: JobStatus,
    task: ParTask<(), EditorJobError>,
    display: Box<dyn FnMut(&imgui::Ui) + Send + 'static>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Complete,
    Error,
}

#[derive(Debug, Error)]
enum EditorJobError {
    #[error("{0}")]
    Error(String),
}

impl EditorJobQueue {
    pub fn add(&mut self, job: EditorJob) {
        self.jobs.push_back(job);
    }

    /// Returns `true` when a job is running.
    pub fn poll(&mut self, ui: &imgui::Ui) -> bool {
        if let Some(job) = &mut self.active_job {
            if job.poll(ui) {
                self.active_job = None;
            }
        }

        if self.active_job.is_none() {
            self.active_job = self.jobs.pop_front();
        }

        self.active_job.is_some()
    }
}

impl EditorJob {
    pub fn new(
        name: &str,
        size: Option<(u32, u32)>,
        task: impl FnOnce() + Send + 'static,
        display: impl FnMut(&imgui::Ui) + Send + 'static,
    ) -> Self {
        let task = ParTask::new(move || {
            task();
            Ok(())
        });

        Self {
            name: name.into(),
            size,
            task,
            status: JobStatus::Running,
            display: Box::new(display),
        }
    }

    /// Should return `true` when the job is complete and should be terminated.
    pub fn poll(&mut self, ui: &imgui::Ui) -> bool {
        let mut window = ui
            .window(&self.name)
            .flags(imgui::WindowFlags::NO_DOCKING)
            .title_bar(false);

        if let Some((w, h)) = &self.size {
            window = window.size([*w as f32, *h as f32], imgui::Condition::Always);
        }

        match self.status {
            JobStatus::Running => {
                window.build(|| match self.task.get() {
                    ParTaskGet::Running => {
                        (self.display)(ui);
                    }
                    ParTaskGet::Err(_) => self.status = JobStatus::Error,
                    ParTaskGet::Panic(_) => self.status = JobStatus::Error,
                    ParTaskGet::Ok(_) => self.status = JobStatus::Complete,
                });
            }
            JobStatus::Complete => {}
            JobStatus::Error => {
                let mut opened = true;
                window.opened(&mut opened).build(|| match self.task.get() {
                    ParTaskGet::Err(err) => {
                        ui.text("An error occured:");
                        ui.text(err.to_string());
                    }
                    ParTaskGet::Panic(panic) => {
                        ui.text("An error occured:");
                        ui.text(format!("{:?}", panic));
                    }
                    _ => {}
                });

                if !opened {
                    self.status = JobStatus::Complete;
                }
            }
        }

        self.status == JobStatus::Complete
    }
}
