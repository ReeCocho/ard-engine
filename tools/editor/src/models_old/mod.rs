pub mod assets;
pub mod scene;

pub trait ViewModel {
    type Message;
    type Model<'a>;

    fn update<'a>(&mut self, model: &mut Self::Model<'a>);

    fn apply<'a>(&mut self, model: &mut Self::Model<'a>, msg: Self::Message) -> Self::Message;

    fn undo<'a>(&mut self, model: &mut Self::Model<'a>, msg: Self::Message) -> Self::Message;

    #[inline]
    fn redo<'a>(&mut self, model: &mut Self::Model<'a>, msg: Self::Message) -> Self::Message {
        self.apply(model, msg)
    }
}

pub struct ViewModelInstance<V: ViewModel> {
    pub vm: V,
    pub messages: ViewModelMessages<V>,
}

pub struct ViewModelMessages<V: ViewModel> {
    messages: Vec<V::Message>,
    undo_stack: Vec<V::Message>,
    redo_stack: Vec<V::Message>,
}

impl<V: ViewModel> ViewModelInstance<V> {
    pub fn new(view_model: V) -> Self {
        Self {
            vm: view_model,
            messages: ViewModelMessages {
                messages: Vec::default(),
                undo_stack: Vec::default(),
                redo_stack: Vec::default(),
            },
        }
    }

    pub fn undo<'a>(&mut self, model: &mut V::Model<'a>) {
        if let Some(msg) = self.messages.undo_stack.pop() {
            self.messages.redo_stack.push(self.vm.undo(model, msg));
        }
    }

    pub fn redo<'a>(&mut self, model: &mut V::Model<'a>) {
        if let Some(msg) = self.messages.redo_stack.pop() {
            self.messages.undo_stack.push(self.vm.redo(model, msg));
        }
    }

    pub fn apply<'a>(&mut self, model: &mut V::Model<'a>) {
        if self.messages.messages.is_empty() {
            return;
        }

        self.messages.redo_stack.clear();

        self.messages.messages.drain(..).for_each(|msg| {
            self.messages.undo_stack.push(self.vm.apply(model, msg));
        });

        self.vm.update(model);
    }
}

impl<V: ViewModel> ViewModelMessages<V> {
    #[inline(always)]
    pub fn send(&mut self, msg: V::Message) {
        self.messages.push(msg);
    }
}
