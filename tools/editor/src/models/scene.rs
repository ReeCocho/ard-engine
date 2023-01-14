use crate::editor::EditorViewModels;

use super::ViewModel;
use ard_engine::{
    assets::prelude::*,
    core::prelude::*,
    ecs::prelude::*,
    game::{
        components::{
            renderable::{RenderableData, RenderableSource},
            transform::{Children, Parent, Transform},
        },
        object::{empty::EmptyObject, static_object::StaticObject, GameObject},
        SceneGameObject,
    },
    input::*,
    math::*,
    render::{
        asset::model::NodeData,
        camera::{Camera, CameraClearColor, CameraDescriptor, CameraIbl, CameraShadows},
        prelude::{CubeMapAsset, Factory, ModelAsset},
        renderer::{Model, RenderLayer, RendererSettings},
    },
};
use smallvec::SmallVec;

pub struct SceneViewModel {
    factory: Factory,
    pub assets: Assets,
    pub view_size: (u32, u32),
    pub looking: bool,
    pub roots: Vec<SceneGraphNode>,
    pub camera: Camera,
    pub camera_rotation: Vec3,
    pub camera_entity: Entity,
    pub camera_descriptor: CameraDescriptor,
}

pub struct SceneGraphNode {
    pub ty: SceneGameObject,
    pub entity: Entity,
    pub name: String,
    pub children: Vec<SceneGraphNode>,
}

pub enum SceneViewMessage {
    InstantiateModel {
        model: Handle<ModelAsset>,
        root: Option<Entity>,
    },
}

pub struct SceneViewUpdate<'a> {
    pub dt: f32,
    pub entity_commands: &'a EntityCommands,
    pub assets: &'a Assets,
    pub settings: &'a mut RendererSettings,
    pub input: &'a InputState,
}

#[derive(SystemState)]
pub struct SceneViewSystem;

impl SceneViewModel {
    pub fn new(assets: &Assets, factory: &Factory, commands: &EntityCommands) -> Self {
        // Load in default skybox
        let skybox = assets.load::<CubeMapAsset>(AssetName::new("skyboxes/default.cube"));
        let diffuse_irradiance =
            assets.load::<CubeMapAsset>(AssetName::new("skyboxes/default.dir.cube"));
        let prefiltered_environment =
            assets.load::<CubeMapAsset>(AssetName::new("skyboxes/default.pem.cube"));
        assets.wait_for_load(&skybox);
        assets.wait_for_load(&diffuse_irradiance);
        assets.wait_for_load(&prefiltered_environment);

        // Create scene view camera
        let camera_descriptor = CameraDescriptor {
            position: Vec3::ZERO,
            target: Vec3::Z,
            up: Vec3::Y,
            near: 0.03,
            far: 200.0,
            fov: 80.0,
            order: 0,
            clear_color: CameraClearColor::SkyBox(assets.get(&skybox).unwrap().cube_map.clone()),
            layers: RenderLayer::all(),
            shadows: Some(CameraShadows {
                resolution: 4096,
                cascades: 4,
            }),
            ibl: CameraIbl {
                diffuse_irradiance: Some(assets.get(&diffuse_irradiance).unwrap().cube_map.clone()),
                prefiltered_environment: Some(
                    assets
                        .get(&prefiltered_environment)
                        .unwrap()
                        .cube_map
                        .clone(),
                ),
            },
            ao: true,
        };

        let camera = factory.create_camera(camera_descriptor.clone());
        let mut camera_entity = [Entity::null()];
        commands.create((vec![camera.clone()],), &mut camera_entity);

        Self {
            assets: assets.clone(),
            factory: factory.clone(),
            view_size: (128, 128),
            looking: false,
            roots: Vec::default(),
            camera,
            camera_rotation: Vec3::ZERO,
            camera_entity: camera_entity[0],
            camera_descriptor,
        }
    }

    fn remove_node(&mut self, entity: Entity) -> Option<SceneGraphNode> {
        fn find_remove(nodes: &mut Vec<SceneGraphNode>, entity: Entity) -> Option<SceneGraphNode> {
            // Search for entity
            let mut idx = None;
            for (i, node) in nodes.iter().enumerate() {
                if node.entity == entity {
                    idx = Some(i);
                    break;
                }
            }

            match idx {
                // Entity found. Remove and return
                Some(i) => Some(nodes.remove(i)),
                // Entity not found. Search children
                None => {
                    for node in nodes {
                        let to_remove = find_remove(&mut node.children, entity);
                        if to_remove.is_some() {
                            return to_remove;
                        }
                    }

                    None
                }
            }
        }

        find_remove(&mut self.roots, entity)
    }
}

impl SceneViewSystem {
    pub fn tick(
        &mut self,
        evt: Tick,
        _: Commands,
        queries: Queries<()>,
        res: Res<(Write<EditorViewModels>, Read<InputState>)>,
    ) {
        let dt = evt.0.as_secs_f32();
        let mut vms = res.get_mut::<EditorViewModels>().unwrap();
        let mut scene = &mut vms.scene.vm;
        let input = res.get::<InputState>().unwrap();

        const LOOK_SPEED: f32 = 0.1;
        const VERTICAL_CLAMP: f32 = 85.0;
        const MOVE_SPEED: f32 = 8.0;

        // Rotate the camera
        if scene.looking {
            let (mx, my) = input.mouse_delta();
            scene.camera_rotation.x += (my as f32) * LOOK_SPEED;
            scene.camera_rotation.y += (mx as f32) * LOOK_SPEED;
            scene.camera_rotation.x = scene
                .camera_rotation
                .x
                .clamp(-VERTICAL_CLAMP, VERTICAL_CLAMP);
        }

        // Direction from rotation
        let rot = Mat4::from_euler(
            EulerRot::YXZ,
            scene.camera_rotation.y.to_radians(),
            scene.camera_rotation.x.to_radians(),
            0.0,
        );

        // Move the camera
        let right = rot.col(0);
        let up = rot.col(1);
        let forward = rot.col(2);

        if input.key(Key::W) {
            scene.camera_descriptor.position += forward.xyz() * dt * MOVE_SPEED;
        }

        if input.key(Key::S) {
            scene.camera_descriptor.position -= forward.xyz() * dt * MOVE_SPEED;
        }

        if input.key(Key::A) {
            scene.camera_descriptor.position -= right.xyz() * dt * MOVE_SPEED;
        }

        if input.key(Key::D) {
            scene.camera_descriptor.position += right.xyz() * dt * MOVE_SPEED;
        }

        // Update the camera
        scene.camera_descriptor.target = scene.camera_descriptor.position + forward.xyz();
        scene.camera_descriptor.up = up.xyz();
        scene
            .factory
            .update_camera(&scene.camera, scene.camera_descriptor.clone());
    }
}

impl ViewModel for SceneViewModel {
    type Message = SceneViewMessage;
    type Model<'a> = SceneViewUpdate<'a>;

    fn update<'a>(&mut self, model: &mut Self::Model<'a>) {}

    fn apply<'a>(&mut self, res: &mut Self::Model<'a>, msg: Self::Message) -> Self::Message {
        match msg {
            SceneViewMessage::InstantiateModel { model, .. } => {
                // This should be handled by the drag/drop task, but is here just in case
                res.assets.wait_for_load(&model);
                let asset = res.assets.get(&model).unwrap();

                let root = instantiate_model(&model, &asset, res.entity_commands);
                let entity = root.entity;
                std::mem::drop(asset);

                self.roots.push(root);

                SceneViewMessage::InstantiateModel {
                    model,
                    root: Some(entity),
                }
            }
        }
    }

    fn undo<'a>(&mut self, model: &mut Self::Model<'a>, msg: Self::Message) -> Self::Message {
        msg
    }
}

impl Into<System> for SceneViewSystem {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(SceneViewSystem::tick)
            .build()
    }
}

fn instantiate_model(
    model_handle: &Handle<ModelAsset>,
    model_asset: &ModelAsset,
    commands: &EntityCommands,
) -> SceneGraphNode {
    #[derive(Default)]
    struct InstancePack {
        empty: EmptyObject,
        empty_entities: Vec<Entity>,
        stat: StaticObject,
        stat_entities: Vec<Entity>,
    }

    // Create empty entities to fill in
    let mut entities = vec![Entity::null(); model_asset.node_count];
    commands.create_empty(&mut entities);

    // Create root scene graph node
    let mut root = SceneGraphNode {
        ty: SceneGameObject::EmptyObject,
        entity: EmptyObject::create_default(commands),
        name: String::from("Root"),
        children: Vec::default(),
    };

    fn instantiate(
        model_handle: &Handle<ModelAsset>,
        model_asset: &ModelAsset,
        node: &ard_engine::render::asset::model::Node,
        parent: Option<Entity>,
        parent_mdl: Mat4,
        pack: &mut InstancePack,
        commands: &EntityCommands,
        entities: &[Entity],
        entity_offset: &mut usize,
    ) -> SceneGraphNode {
        // Grab the entity we are going to use for this component
        debug_assert!(*entity_offset < entities.len());
        let entity = entities[*entity_offset];
        *entity_offset += 1;

        let mut scene_node = SceneGraphNode {
            // Empty by default until we determine the correct type
            ty: SceneGameObject::EmptyObject,
            children: Vec::with_capacity(node.children.len()),
            entity,
            name: node.name.clone(),
        };

        // Create the components common to all game object types
        let model = Model(parent_mdl * node.model);

        let transform = {
            let scale = Vec3::new(
                node.model.col(0).xyz().length(),
                node.model.col(1).xyz().length(),
                node.model.col(2).xyz().length(),
            );

            let mut rot_mat = node.model;
            *rot_mat.col_mut(0) /= scale.x;
            *rot_mat.col_mut(1) /= scale.y;
            *rot_mat.col_mut(2) /= scale.z;

            Transform {
                position: Vec3A::ZERO,    // node.model.col(3).xyz().into(),
                rotation: Quat::IDENTITY, // Quat::from_mat4(&rot_mat),
                scale: Vec3A::ONE,        // scale.into(),
            }
        };

        let parent = Parent(parent);

        let mut children = Children(SmallVec::with_capacity(node.children.len()));

        // Create all of our children
        for child in &node.children {
            scene_node.children.push(instantiate(
                model_handle,
                model_asset,
                child,
                Some(entity),
                model.0,
                pack,
                commands,
                entities,
                entity_offset,
            ));
            children.0.push(scene_node.children.last().unwrap().entity);
        }

        // Determine object type and add to the appropriate pack
        match &node.data {
            NodeData::Empty => {
                scene_node.ty = SceneGameObject::EmptyObject;
                pack.empty.field_Children.push(children);
                pack.empty.field_Parent.push(parent);
                pack.empty.field_Transform.push(transform);
                pack.empty.field_Model.push(model);
                pack.empty_entities.push(entity);
            }
            // If there is only one mesh instance, we can turn the entity into a renderable
            NodeData::MeshGroup(idx) => {
                if model_asset.mesh_groups[*idx].0.len() == 1 {
                    scene_node.ty = SceneGameObject::StaticObject;
                    pack.stat.field_Children.push(children);
                    pack.stat.field_Parent.push(parent);
                    pack.stat.field_Transform.push(transform);
                    pack.stat.field_Model.push(model);
                    pack.stat.field_RenderableData.push(RenderableData {
                        source: Some(RenderableSource::Model {
                            model: model_handle.clone(),
                            mesh_group_idx: *idx,
                            mesh_idx: 0,
                        }),
                    });
                    pack.stat_entities.push(entity);
                }
                // Otherwise, it must be a parent to multiple other objects
                else {
                    let mesh_group = &model_asset.mesh_groups[*idx].0;

                    let mut mesh_instances = StaticObject {
                        field_Children: vec![Children(SmallVec::default()); mesh_group.len()],
                        field_Model: vec![model; mesh_group.len()],
                        field_Parent: vec![Parent(Some(entity)); mesh_group.len()],
                        field_Transform: vec![
                            Transform {
                                position: Vec3A::ZERO,
                                scale: Vec3A::ONE,
                                rotation: Quat::IDENTITY,
                            };
                            mesh_group.len()
                        ],
                        field_RenderableData: Vec::with_capacity(mesh_group.len()),
                    };

                    for i in 0..mesh_group.len() {
                        mesh_instances.field_RenderableData.push(RenderableData {
                            source: Some(RenderableSource::Model {
                                model: model_handle.clone(),
                                mesh_group_idx: *idx,
                                mesh_idx: i,
                            }),
                        })
                    }

                    let start = children.0.len();
                    children
                        .0
                        .extend(std::iter::repeat(Entity::null()).take(mesh_group.len()));
                    commands.create(
                        (
                            mesh_instances.field_Children,
                            mesh_instances.field_Model,
                            mesh_instances.field_Parent,
                            mesh_instances.field_Transform,
                            mesh_instances.field_RenderableData,
                        ),
                        &mut children.0[start..],
                    );

                    for child in &children.0[start..] {
                        scene_node.children.push(SceneGraphNode {
                            ty: SceneGameObject::StaticObject,
                            name: String::from("mesh instance"),
                            entity: *child,
                            children: Vec::default(),
                        })
                    }

                    scene_node.ty = SceneGameObject::EmptyObject;
                    pack.empty.field_Children.push(children);
                    pack.empty.field_Parent.push(parent);
                    pack.empty.field_Transform.push(transform);
                    pack.empty.field_Model.push(model);
                    pack.empty_entities.push(entity);
                }
            }
            NodeData::Light(_) => todo!(),
        }

        scene_node
    }

    // Instantiate each model root recursively
    let mut pack = InstancePack::default();
    let mut entity_offset = 0;

    for node in &model_asset.roots {
        root.children.push(instantiate(
            model_handle,
            model_asset,
            node,
            Some(root.entity),
            Mat4::IDENTITY,
            &mut pack,
            commands,
            &entities,
            &mut entity_offset,
        ));
    }

    // Set the components of the created entities
    commands.set_components(
        &pack.empty_entities,
        (
            pack.empty.field_Children,
            pack.empty.field_Model,
            pack.empty.field_Parent,
            pack.empty.field_Transform,
        ),
    );

    commands.set_components(
        &pack.stat_entities,
        (
            pack.stat.field_Children,
            pack.stat.field_Model,
            pack.stat.field_Parent,
            pack.stat.field_RenderableData,
            pack.stat.field_Transform,
        ),
    );

    root
}
