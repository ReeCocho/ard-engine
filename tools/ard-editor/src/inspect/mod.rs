pub mod stat;
pub mod transform;

use ard_engine::ecs::prelude::*;
use stat::StaticInspector;

#[derive(Default)]
pub struct Inspectors {
    static_inspector: StaticInspector,
    inspectors: Vec<Box<dyn Inspector>>,
}

pub struct InspectorContext<'a> {
    pub ui: &'a mut egui::Ui,
    pub entity: Entity,
    pub commands: &'a Commands,
    pub queries: &'a Queries<Everything>,
    pub res: &'a Res<Everything>,
}

pub trait Inspector {
    fn should_inspect(&self, ctx: InspectorContext) -> bool;

    fn title(&self) -> &'static str;

    fn show(&mut self, ctx: InspectorContext);
}

impl Inspectors {
    pub fn with(&mut self, inspector: impl Inspector + 'static) {
        self.inspectors.push(Box::new(inspector));
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        entity: Entity,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        self.static_inspector.show(InspectorContext {
            ui,
            entity,
            commands,
            queries,
            res,
        });

        self.inspectors.iter_mut().for_each(|inspector| {
            if !inspector.should_inspect(InspectorContext {
                ui,
                entity,
                commands,
                queries,
                res,
            }) {
                return;
            }

            ui.collapsing(inspector.title(), |ui| {
                inspector.show(InspectorContext {
                    ui,
                    entity,
                    commands,
                    queries,
                    res,
                });
            });
        });
    }
}
