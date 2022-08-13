use ard_engine::{
    assets::prelude::*, core::prelude::*, ecs::prelude::*, graphics::prelude::*, input::InputState,
    window::prelude::*,
};

use crate::{
    controller::Controller,
    scene_graph::{SceneGraph, SceneGraphAsset, SceneGraphLoader},
    util::{
        asset_meta::{AssetMeta, AssetMetaLoader},
        dirty_assets::DirtyAssets,
        editor_job::EditorJobQueue,
    },
    view::{
        assets::AssetViewer,
        hierarchy::Hierarchy,
        inspector::{Inspector, InspectorItem},
        lighting::LightingGui,
        scene_view::{SceneView, SceneViewCamera},
        toolbar::ToolBar,
        View,
    },
};

#[derive(SystemState)]
pub struct Editor {
    controller: Controller,
    jobs: EditorJobQueue,
    dirty: DirtyAssets,
    toolbar: ToolBar,
    scene_view: SceneView,
    asset_viewer: AssetViewer,
    inspector: Inspector,
    hierarchy: Hierarchy,
    lighting: LightingGui,
}

type EditorResources = (
    Read<Factory>,
    Read<InputState>,
    Write<Windows>,
    Write<DebugGui>,
    Write<RendererSettings>,
    Write<Assets>,
    Write<SceneGraph>,
    Write<Lighting>,
    Write<DebugDrawing>,
    Write<SceneViewCamera>,
);

pub struct Resources<'a> {
    pub dt: f32,
    pub ecs_commands: Commands,
    pub queries: &'a Queries<Everything>,
    pub factory: &'a Factory,
    pub input: &'a InputState,
    pub dirty: &'a mut DirtyAssets,
    pub jobs: &'a mut EditorJobQueue,
    pub windows: &'a mut Windows,
    pub debug_draw: &'a mut DebugDrawing,
    pub renderer_settings: &'a mut RendererSettings,
    pub assets: &'a mut Assets,
    pub scene_graph: &'a mut SceneGraph,
    pub camera: &'a mut SceneViewCamera,
    pub lighting: &'a mut Lighting,
}

impl Editor {
    pub fn setup(app: &mut App) {
        // Register meta loader
        let assets = app.resources.get::<Assets>().unwrap();
        assets.register::<AssetMeta>(AssetMetaLoader);
        assets.register::<SceneGraphAsset>(SceneGraphLoader);

        // Change render settings
        let mut settings = app.resources.get_mut::<RendererSettings>().unwrap();

        // Don't use the surface size
        settings.canvas_size = Some((100, 100));

        // Disable frame rate limit
        settings.render_time = None;

        // Don't render the scene to the surface
        settings.render_scene = false;

        // Add the editor system
        app.dispatcher.add_system(Editor {
            controller: Controller::default(),
            jobs: EditorJobQueue::default(),
            dirty: DirtyAssets::default(),
            toolbar: ToolBar::default(),
            scene_view: SceneView::default(),
            asset_viewer: AssetViewer::new(&assets),
            inspector: Inspector::new(),
            hierarchy: Hierarchy::default(),
            lighting: LightingGui,
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
        self.asset_viewer.import(&evt.file, &assets);
    }

    fn inspect_item(
        &mut self,
        item: InspectorItem,
        _: Commands,
        _: Queries<()>,
        res: Res<(Read<Assets>, Read<SceneGraph>)>,
    ) {
        let res = res.get();
        let assets = res.0.unwrap();
        let scene_graph = res.1.unwrap();

        self.inspector
            .set_inspected_item(&assets, &scene_graph, Some(item));
    }

    fn entity_image(
        &mut self,
        _: NewEntityImage,
        _: Commands,
        _: Queries<()>,
        res: Res<(Read<EntityImage>, Read<SceneGraph>, Read<Assets>)>,
    ) {
        let res = res.get();
        let entity_image = res.0.unwrap();
        let scene_graph = res.1.unwrap();
        let assets = res.2.unwrap();

        if self.scene_view.clicked() {
            // Sample the entity
            let entity = entity_image.sample(self.scene_view.click_uv());

            // If we clicked something, inspect it
            if let Some(entity) = entity {
                self.inspector.set_inspected_item(
                    &assets,
                    &scene_graph,
                    Some(InspectorItem::Entity(entity)),
                );
            }

            self.scene_view.reset_click();
        }
    }

    fn pre_render(
        &mut self,
        evt: PreRender,
        commands: Commands,
        queries: Queries<Everything>,
        res: Res<EditorResources>,
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
        let mut lighting = res.7.unwrap();
        let mut debug_drawing = res.8.unwrap();
        let mut camera = res.9.unwrap();

        let mut resources = Resources {
            dt,
            ecs_commands: commands,
            queries: &queries,
            factory: &factory,
            input: &input,
            dirty: &mut self.dirty,
            jobs: &mut self.jobs,
            windows: &mut windows,
            debug_draw: &mut debug_drawing,
            renderer_settings: &mut settings,
            assets: &mut assets,
            scene_graph: &mut scene_graph,
            lighting: &mut lighting,
            camera: &mut camera,
        };

        gui.begin_dock();
        let ui = gui.ui();

        let disabled = resources.jobs.poll(ui);
        ui.disabled(disabled, || {
            resources.scene_graph.receive_nodes();
            resources
                .scene_graph
                .update_active_scene(&resources.assets, &resources.ecs_commands.entities);

            self.toolbar.show(ui, &mut self.controller, &mut resources);
            self.scene_view
                .show(ui, &mut self.controller, &mut resources);
            self.asset_viewer
                .show(ui, &mut self.controller, &mut resources);
            self.inspector
                .show(ui, &mut self.controller, &mut resources);
            self.hierarchy
                .show(ui, &mut self.controller, &mut resources);
            self.lighting.show(ui, &mut self.controller, &mut resources);
        });
    }
}

impl Into<System> for Editor {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(Editor::entity_image)
            .with_handler(Editor::file_dropped)
            .with_handler(Editor::pre_render)
            .with_handler(Editor::inspect_item)
            .build()
    }
}
