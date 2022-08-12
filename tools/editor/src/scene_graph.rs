use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    game::{
        components::transform::Parent,
        destroy::Destroy,
        lighting::LightingSettings,
        object::{empty::EmptyObject, static_object::StaticObject, GameObject},
        scene::{EntityMap, MappedEntity},
        Scene, SceneDescriptor, SceneEntities, SceneGameObject,
    },
    log::{info, warn},
};
use async_trait::async_trait;
use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

#[derive(Resource)]
pub struct SceneGraph {
    /// Handle to the scene asset for this graph.
    handle: Option<Handle<SceneGraphAsset>>,
    /// Lighting settings for the scene.
    lighting: LightingSettings,
    /// Root nodes in the graph.
    roots: Vec<SceneGraphNode>,
    // Channel for creating new nodes.
    new_node_send: Sender<SceneGraphNode>,
    new_node_recv: Receiver<SceneGraphNode>,
    // Channel for changing the scene. If the boolean flag is set to true, the scene will be
    // loaded. If it is false, the handle will simply be updated.
    load_scene_send: Sender<(Handle<SceneGraphAsset>, bool)>,
    load_scene_recv: Receiver<(Handle<SceneGraphAsset>, bool)>,
}

pub struct SceneGraphNode {
    pub entity: Entity,
    pub children: Vec<SceneGraphNode>,
    pub ty: SceneGameObject,
}

#[derive(Default, Serialize, Deserialize)]
pub struct SceneGraphDescriptor {
    pub nodes: Vec<SceneGraphNodeDescriptor>,
    pub scene: SceneDescriptor,
}

#[derive(Serialize, Deserialize)]
pub struct SceneGraphNodeDescriptor {
    pub entity: MappedEntity,
    pub children: Vec<SceneGraphNodeDescriptor>,
    pub ty: SceneGameObject,
}

pub struct SceneGraphAsset {
    pub descriptor: Option<SceneGraphDescriptor>,
}

pub struct SceneGraphLoader;

impl Asset for SceneGraphAsset {
    const EXTENSION: &'static str = "scene";

    type Loader = SceneGraphLoader;
}

impl Default for SceneGraph {
    fn default() -> Self {
        let (new_node_send, new_node_recv) = crossbeam_channel::unbounded();
        let (load_scene_send, load_scene_recv) = crossbeam_channel::unbounded();

        Self {
            handle: None,
            lighting: LightingSettings::default(),
            roots: Vec::default(),
            new_node_send,
            new_node_recv,
            load_scene_send,
            load_scene_recv,
        }
    }
}

impl SceneGraph {
    #[inline]
    pub fn asset(&self) -> Option<&Handle<SceneGraphAsset>> {
        self.handle.as_ref()
    }

    #[inline]
    pub fn roots(&self) -> &[SceneGraphNode] {
        &self.roots
    }

    #[inline]
    pub fn lighting(&self) -> &LightingSettings {
        &self.lighting
    }

    #[inline]
    pub fn lighting_mut(&mut self) -> &mut LightingSettings {
        &mut self.lighting
    }

    /// # Note
    /// All nodes sent through this channel must not have a parent.
    #[inline]
    pub fn new_node_channel(&self) -> Sender<SceneGraphNode> {
        self.new_node_send.clone()
    }

    #[inline]
    pub fn load_scene_channel(&self) -> Sender<(Handle<SceneGraphAsset>, bool)> {
        self.load_scene_send.clone()
    }

    pub fn receive_nodes(&mut self) {
        while let Ok(new_node) = self.new_node_recv.try_recv() {
            self.roots.push(new_node);
        }
    }

    pub fn update_active_scene(&mut self, assets: &Assets, commands: &EntityCommands) {
        while let Ok((scene, load)) = self.load_scene_recv.try_recv() {
            if load {
                info!("loading new scene...");

                // If the scene to load is the same as the current scene, we must reload it
                if Some(scene.clone()) == self.handle {
                    assets.reload(&scene);
                }

                // Wait for the scene to load
                assets.wait_for_load(&scene);

                // Grab the scene asset
                let mut scene_asset = match assets.get_mut(&scene) {
                    Some(scene) => scene,
                    None => {
                        warn!("could not load scene");
                        continue;
                    }
                };

                let descriptor = match scene_asset.descriptor.take() {
                    Some(descriptor) => descriptor,
                    None => {
                        warn!("attempt to load scene, but descriptor was `None`");
                        continue;
                    }
                };

                std::mem::drop(scene_asset);

                self.load(descriptor, commands, assets);
                self.handle = Some(scene);
                info!("new scene loaded...");
            }
        }
    }

    pub fn find_entity(&self, entity: Entity) -> Option<&SceneGraphNode> {
        for root in &self.roots {
            let search = self.find_entity_recurse(entity, root);
            if search.is_some() {
                return search;
            }
        }

        None
    }

    /// Attempts to set the parent of the entity. If a circular dependency would be created, the
    /// parent value is not updated.
    pub fn set_parent(
        &mut self,
        entity: Entity,
        new_parent: Option<Entity>,
        queries: &Queries<Everything>,
        commands: &Commands,
    ) {
        // First, find the node of the entity
        let mut entity_node = None;
        for root in &self.roots {
            let search = self.find_entity_recurse(entity, root);
            if search.is_some() {
                entity_node = search;
                break;
            }
        }

        let entity_node = match entity_node {
            Some(node) => node,
            None => return,
        };

        // Now, make sure the parent is not a child of the node
        if let Some(new_parent) = new_parent {
            if self.find_entity_recurse(new_parent, &entity_node).is_some() {
                return;
            }
        }

        // If it is not, remove the entity node from the graph
        let node = self.remove_entity(entity).unwrap();

        // Then find the parent
        let parent = match new_parent {
            Some(new_parent) => match self.find_entity_mut(new_parent) {
                Some(parent) => Some(parent),
                None => return,
            },
            None => None,
        };

        // Update the parent component in the ECS
        let mut entity_parent_comp = queries.get::<Write<Parent>>(entity).unwrap();
        entity_parent_comp.set(entity, new_parent, commands);
        match parent {
            Some(parent) => parent.children.push(node),
            None => self.roots.push(node),
        }
    }

    pub fn save(&self, queries: &Queries<Everything>, assets: &Assets) -> SceneGraphDescriptor {
        let mut entities = SceneEntities::default();

        fn add_nodes_entities(entities: &mut SceneEntities, node: &SceneGraphNode) {
            // Add the entity to the correct list
            match &node.ty {
                SceneGameObject::StaticObject => entities.StaticObject_entities.push(node.entity),
                SceneGameObject::EmptyObject => entities.EmptyObject_entities.push(node.entity),
            }

            // Traverse children
            for child in &node.children {
                add_nodes_entities(entities, child);
            }
        }

        for root in &self.roots {
            add_nodes_entities(&mut entities, root);
        }

        // Create the descriptor
        let (descriptor, mapping) = SceneDescriptor::new(entities, &self.lighting, queries, assets);

        // Create the scene graph descriptor from the mapping
        let mut sg_descriptor = SceneGraphDescriptor::default();
        sg_descriptor.nodes = Vec::with_capacity(self.roots.len());
        sg_descriptor.scene = descriptor;

        fn create_sg_node_descriptor(
            mapping: &EntityMap,
            node: &SceneGraphNode,
        ) -> SceneGraphNodeDescriptor {
            let mut ret = SceneGraphNodeDescriptor {
                children: Vec::with_capacity(node.children.len()),
                entity: mapping.to_map(node.entity),
                ty: node.ty,
            };

            for child in &node.children {
                ret.children.push(create_sg_node_descriptor(mapping, child));
            }

            ret
        }

        for root in &self.roots {
            sg_descriptor
                .nodes
                .push(create_sg_node_descriptor(&mapping, root));
        }

        sg_descriptor
    }

    pub fn load(
        &mut self,
        mut descriptor: SceneGraphDescriptor,
        commands: &EntityCommands,
        assets: &Assets,
    ) {
        // Destroy every entity currently in the scene
        fn destroy(node: &SceneGraphNode, commands: &EntityCommands) {
            commands.add_component(node.entity, Destroy);

            for child in &node.children {
                destroy(child, commands);
            }
        }

        for root in &self.roots {
            destroy(root, commands);
        }

        self.roots.clear();

        // Lighting settings
        self.lighting =
            std::mem::take(&mut descriptor.scene.lighting_settings).into_settings(assets);

        // Load in the provided descriptor
        let map = descriptor.scene.load(&commands, assets);

        // Construct the scene graph from the mapping
        fn construct_node(
            descriptor: &SceneGraphNodeDescriptor,
            mapping: &EntityMap,
        ) -> SceneGraphNode {
            let mut node = SceneGraphNode {
                entity: mapping.from_map(descriptor.entity),
                children: Vec::with_capacity(descriptor.children.len()),
                ty: descriptor.ty,
            };

            for child in &descriptor.children {
                node.children.push(construct_node(child, mapping));
            }

            node
        }

        for node in &descriptor.nodes {
            self.roots.push(construct_node(node, &map));
        }
    }

    pub fn create(&mut self, ty: SceneGameObject, commands: &EntityCommands) {
        let entity = match ty {
            SceneGameObject::StaticObject => StaticObject::create_default(commands),
            SceneGameObject::EmptyObject => EmptyObject::create_default(commands),
        };

        self.roots.push(SceneGraphNode {
            entity,
            children: Vec::default(),
            ty,
        });
    }

    fn remove_entity(&mut self, entity: Entity) -> Option<SceneGraphNode> {
        fn remove_entity_recurse(
            entity: Entity,
            node: &mut SceneGraphNode,
        ) -> Option<SceneGraphNode> {
            let mut idx = None;
            for (i, node) in node.children.iter().enumerate() {
                if node.entity == entity {
                    idx = Some(i);
                    break;
                }
            }

            match idx {
                Some(idx) => Some(node.children.remove(idx)),
                None => {
                    for child in &mut node.children {
                        let res = remove_entity_recurse(entity, child);
                        if res.is_some() {
                            return res;
                        }
                    }

                    None
                }
            }
        }

        for i in 0..self.roots.len() {
            if self.roots[i].entity == entity {
                return Some(self.roots.remove(i));
            }

            let search = remove_entity_recurse(entity, &mut self.roots[i]);
            if search.is_some() {
                return search;
            }
        }

        None
    }

    fn find_entity_recurse<'a>(
        &'a self,
        entity: Entity,
        node: &'a SceneGraphNode,
    ) -> Option<&SceneGraphNode> {
        if node.entity == entity {
            return Some(node);
        }

        for child in &node.children {
            let recurse = self.find_entity_recurse(entity, child);
            if recurse.is_some() {
                return recurse;
            }
        }

        None
    }

    fn find_entity_mut(&mut self, entity: Entity) -> Option<&mut SceneGraphNode> {
        fn find_entity_recurse_mut(
            entity: Entity,
            node: &mut SceneGraphNode,
        ) -> Option<&mut SceneGraphNode> {
            if node.entity == entity {
                return Some(node);
            }

            for child in &mut node.children {
                let found = find_entity_recurse_mut(entity, child);
                if found.is_some() {
                    return found;
                }
            }

            None
        }

        for root in &mut self.roots {
            let found = find_entity_recurse_mut(entity, root);
            if found.is_some() {
                return found;
            }
        }

        None
    }
}

#[async_trait]
impl AssetLoader for SceneGraphLoader {
    type Asset = SceneGraphAsset;

    async fn load(
        &self,
        _: Assets,
        package: Package,
        asset: &AssetName,
    ) -> Result<AssetLoadResult<Self::Asset>, AssetLoadError> {
        // Load in the descriptor
        let descriptor = package.read_str(asset).await?;
        let descriptor = match ron::from_str::<SceneGraphDescriptor>(&descriptor) {
            Ok(descriptor) => descriptor,
            Err(err) => return Err(AssetLoadError::Other(Box::new(err))),
        };

        Ok(AssetLoadResult::Loaded {
            asset: SceneGraphAsset {
                descriptor: Some(descriptor),
            },
            persistent: false,
        })
    }

    async fn post_load(
        &self,
        _: Assets,
        _: Package,
        _: Handle<Self::Asset>,
    ) -> Result<AssetPostLoadResult, AssetLoadError> {
        panic!("post load not needed")
    }
}
