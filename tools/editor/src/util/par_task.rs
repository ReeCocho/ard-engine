use std::{any::Any, error::Error, thread::JoinHandle};

pub struct ParTask<V, E> {
    task: Option<Box<dyn FnOnce() -> Result<V, E> + Send + 'static>>,
    handle: Option<JoinHandle<Result<V, E>>>,
    value: ParTaskInnerValue<V, E>,
}

pub enum ParTaskGet<'a, V, E> {
    Running,
    Err(&'a E),
    Panic(&'a Box<dyn Any + Send + 'static>),
    Ok(&'a mut V),
}

enum ParTaskInnerValue<V, E> {
    None,
    Err(E),
    Panic(Box<dyn Any + Send + 'static>),
    Ok(V),
}

unsafe impl<V: Send + 'static, E: Error + Send + 'static> Sync for ParTask<V, E> {}

impl<V: Send + 'static, E: Error + Send + 'static> Default for ParTask<V, E> {
    #[inline]
    fn default() -> Self {
        Self {
            task: None,
            handle: None,
            value: ParTaskInnerValue::None,
        }
    }
}

impl<V: Send + 'static, E: Error + Send + 'static> ParTask<V, E> {
    #[inline]
    pub fn new(func: impl FnOnce() -> Result<V, E> + Send + 'static) -> Self {
        Self {
            task: Some(Box::new(func)),
            handle: None,
            value: ParTaskInnerValue::None,
        }
    }

    pub fn ui<F: FnOnce(&mut V)>(&mut self, ui: &mut egui::Ui, func: F) {
        match self.get() {
            ParTaskGet::Running => {
                ui.horizontal(|ui| {
                    ui.label("Loading...");
                    ui.spinner();
                });
            }
            ParTaskGet::Err(err) => {
                ui.label("An error has occured:");
                ui.label(format!("{:?}", err));
            }
            ParTaskGet::Panic(panic) => {
                ui.label("A panic has occured:");
                ui.label(format!("{:?}", panic));
            }
            ParTaskGet::Ok(val) => {
                func(val);
            }
        }
    }

    #[inline]
    pub fn has_task(&self) -> bool {
        self.task.is_some()
            || self.handle.is_some()
            || match &self.value {
                ParTaskInnerValue::None => false,
                _ => true,
            }
    }

    #[inline]
    pub fn get(&mut self) -> ParTaskGet<V, E> {
        // Check if we have a result from the task
        match self.value {
            ParTaskInnerValue::Ok(ref mut val) => return ParTaskGet::Ok(val),
            ParTaskInnerValue::Err(ref err) => return ParTaskGet::Err(err),
            ParTaskInnerValue::Panic(ref panic) => return ParTaskGet::Panic(panic),
            ParTaskInnerValue::None => {}
        }

        // Check if we need to spawn the task
        if self.handle.is_none() {
            let task = self.task.take().expect("no task to run");
            self.handle = Some(std::thread::spawn(|| task()));
        }

        // Poll result from the task
        let handle = self.handle.as_mut().unwrap();
        if handle.is_finished() {
            match self.handle.take().unwrap().join() {
                Ok(res) => match res {
                    Ok(val) => {
                        self.value = ParTaskInnerValue::Ok(val);
                    }
                    Err(err) => {
                        self.value = ParTaskInnerValue::Err(err);
                    }
                },
                Err(panic) => {
                    self.value = ParTaskInnerValue::Panic(panic);
                }
            }

            self.get()
        } else {
            ParTaskGet::Running
        }
    }
}
