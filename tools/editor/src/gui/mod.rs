pub mod assets;
pub mod dirty_assets;
pub mod hierarchy;
pub mod inspector;
pub mod scene_view;
pub mod toolbar;
pub mod util;

use ard_engine::{
    assets::prelude::*, core::prelude::*, ecs::prelude::*, graphics::prelude::*, input::*,
    window::prelude::*,
};

use assets::*;
use scene_view::*;

use crate::{editor_job::EditorJobQueue, scene_graph::SceneGraph};

use self::{
    dirty_assets::DirtyAssets,
    hierarchy::Hierarchy,
    inspector::{Inspector, InspectorItem},
    toolbar::ToolBar,
};

#[derive(SystemState)]
pub struct Editor {
    dirty: DirtyAssets,
    jobs: EditorJobQueue,
    tool_bar: ToolBar,
    scene_view: SceneView,
    hierarchy: Hierarchy,
    assets: AssetViewer,
    inspector: Inspector,
}

type EditorGuiResources = (
    Read<Factory>,
    Read<InputState>,
    Write<Windows>,
    Write<DebugGui>,
    Write<RendererSettings>,
    Write<Assets>,
    Write<SceneGraph>,
);

impl Editor {
    pub fn startup(app: &mut App) {
        let assets = app.resources.get::<Assets>().unwrap();
        app.dispatcher.add_system(Editor {
            dirty: DirtyAssets::default(),
            jobs: EditorJobQueue::default(),
            tool_bar: ToolBar::default(),
            scene_view: SceneView::default(),
            hierarchy: Hierarchy::default(),
            assets: AssetViewer::new(&assets),
            inspector: Inspector::new(),
        });
    }

    fn file_dropped(
        &mut self,
        evt: WindowFileDropped,
        _: Commands,
        _: Queries<()>,
        res: Res<(Read<Assets>,)>,
    ) {
        let res = res.get();
        let assets = res.0.unwrap();
        self.assets.import(&evt.file, &assets);
    }

    fn inspect_item(
        &mut self,
        item: InspectorItem,
        _: Commands,
        _: Queries<()>,
        res: Res<(Read<Assets>,)>,
    ) {
        self.inspector
            .set_inspected_item(&res.get().0.unwrap(), Some(item));
    }

    fn pre_render(
        &mut self,
        evt: PreRender,
        commands: Commands,
        queries: Queries<Everything>,
        res: Res<EditorGuiResources>,
    ) {
        let dt = evt.0.as_secs_f32();

        let res = res.get();
        let factory = res.0.unwrap();
        let input = res.1.unwrap();
        let mut windows = res.2.unwrap();
        let mut gui = res.3.unwrap();
        let mut settings = res.4.unwrap();
        let mut assets = res.5.unwrap();
        let mut scene_graph = res.6.unwrap();

        gui.begin_dock();
        let ui = gui.ui();

        let disabled = self.jobs.poll(ui);
        ui.disabled(disabled, || {
            self.tool_bar.draw(
                ui,
                &queries,
                &mut scene_graph,
                &assets,
                &commands.entities,
                &mut self.dirty,
                &mut self.jobs,
            );
            self.scene_view
                .draw(dt, &factory, &input, &mut windows, ui, &mut settings);
            self.assets.draw(ui, &assets, &commands);
            self.inspector
                .draw(ui, &mut assets, &mut self.dirty, &factory);
            self.hierarchy.draw(ui, &scene_graph);
        });
    }
}

impl Into<System> for Editor {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(Editor::file_dropped)
            .with_handler(Editor::pre_render)
            .with_handler(Editor::inspect_item)
            .build()
    }
}
