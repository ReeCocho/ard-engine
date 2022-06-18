pub mod asset_meta;
pub mod assets;
pub mod inspector;
pub mod scene_view;

use ard_engine::{
    assets::prelude::*, core::prelude::*, ecs::prelude::*, graphics::prelude::*, input::*,
    window::prelude::*,
};

use assets::*;
use scene_view::*;

use self::inspector::{Inspector, InspectorItem};

#[derive(SystemState)]
pub struct EditorGui {
    scene_view: SceneView,
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
);

impl EditorGui {
    pub fn startup(app: &mut App) {
        let assets = app.resources.get::<Assets>().unwrap();
        app.dispatcher.add_system(EditorGui {
            scene_view: SceneView::default(),
            assets: AssetViewer::new(&assets),
            inspector: Inspector::new(),
        });
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
        _: Queries<()>,
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

        gui.begin_dock();

        let ui = gui.ui();
        self.scene_view
            .draw(dt, &factory, &input, &mut windows, ui, &mut settings);
        self.assets.draw(ui, &commands);
        self.inspector.draw(ui, &mut assets);
    }
}

impl Into<System> for EditorGui {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(EditorGui::pre_render)
            .with_handler(EditorGui::inspect_item)
            .build()
    }
}
