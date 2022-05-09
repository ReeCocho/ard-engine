use std::{any::TypeId, num::NonZeroU32};

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::{
    archetype::{ArchetypeId, Archetypes},
    component::pack::ComponentPack,
    entity::Entity,
    prelude::{Component, ComponentExt},
    tag::{pack::TagPack, Tag, TagCollectionId, TagExt, Tags},
};

/// Container for entities belonging to a world.
pub struct Entities {
    /// List of entity descriptions.
    entities: Vec<EntityInfo>,
    /// List of free entity IDs.
    free: Vec<u32>,
    /// List of entities to create when processing.
    to_create: Vec<EntityCreateInfo>,
    /// List of entities to destroy when processing.
    to_destroy: Vec<Entity>,
    sender: Sender<EntityCommand>,
    receiver: Receiver<EntityCommand>,
}

/// Used to manipulated created entities.
#[derive(Clone)]
pub struct EntityCommands {
    sender: Sender<EntityCommand>,
}

pub(crate) enum EntityCommand {
    Create {
        components: Box<dyn ComponentPack>,
        tags: Option<Box<dyn TagPack>>,
    },
    Destroy {
        entities: Vec<Entity>,
    },
    RemoveComponent {
        entity: Entity,
        id: TypeId,
    },
    AddComponent {
        entity: Entity,
        component: Box<dyn ComponentExt>,
    },
    RemoveTag {
        entity: Entity,
        id: TypeId,
    },
    AddTag {
        entity: Entity,
        tag: Box<dyn TagExt>,
    },
}

/// Description of an entity within the world.
#[derive(Debug, Copy, Clone)]
pub(crate) struct EntityInfo {
    /// Current version of the entity.
    pub ver: NonZeroU32,
    /// ID of the archetype the entities components exist in.
    pub archetype: ArchetypeId,
    /// Index within each archetype buffer the entities components exists in.
    /// NOTE: You might wonder "why not a usize"? By making this a u32 we can get the size of
    /// this struct down to 16 from 24 which, imo, is a worthy compromise seeing as having more
    /// than 4 billion entities in a single archetype should (hopefully) never happen.
    pub index: u32,
    /// Index of the collection the entities tags exist in or `None` if the entity has no tags.
    pub collection: Option<TagCollectionId>,
}

/// A pack of data used to create a set of entities.
struct EntityCreateInfo {
    /// Component pack used to initialize the entities.  
    components: Box<dyn ComponentPack>,
    /// Optional tag pack used to initialize the entities.
    tags: Option<Box<dyn TagPack>>,
    /// Handles of the generated entities.
    entities: Vec<Entity>,
}

impl Default for Entities {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        Self {
            sender,
            receiver,
            entities: Vec::default(),
            free: Vec::default(),
            to_create: Vec::default(),
            to_destroy: Vec::default(),
        }
    }
}

impl Entities {
    /// Determines the number of active entities.
    #[inline]
    pub fn count(&self) -> usize {
        self.entities.len() - self.free.len() - self.to_destroy.len()
    }

    pub fn commands(&self) -> EntityCommands {
        EntityCommands {
            sender: self.sender.clone(),
        }
    }

    /// Process entities pending creation an deletion.
    pub fn process(&mut self, archetypes: &mut Archetypes, tags: &mut Tags) {
        // Process commands
        for command in self.receiver.clone().try_iter() {
            match command {
                EntityCommand::Create { components, tags } => {
                    self.create_with_tags_dyn(components, tags);
                }
                EntityCommand::Destroy { entities } => {
                    self.destroy(&entities);
                }
                EntityCommand::RemoveComponent { entity, id } => {
                    let mut info = &mut self.entities[entity.id() as usize];
                    assert!(info.ver.get() == entity.ver());
                    if let Some((new_archetype, new_index, moved_entity)) =
                        archetypes.remove_component(entity, info.archetype, info.index as usize, id)
                    {
                        let old_idx = info.index;
                        info.archetype = new_archetype;
                        info.index = new_index as u32;

                        if let Some(moved_entity) = moved_entity {
                            self.entities[moved_entity.id() as usize].index = old_idx;
                        }
                    }
                }
                EntityCommand::AddComponent { entity, component } => {
                    let mut info = &mut self.entities[entity.id() as usize];
                    assert!(info.ver.get() == entity.ver());
                    if let Some((new_archetype, new_index, moved_entity)) = archetypes
                        .add_component(entity, info.archetype, info.index as usize, component)
                    {
                        let old_idx = info.index;
                        info.archetype = new_archetype;
                        info.index = new_index as u32;

                        if let Some(moved_entity) = moved_entity {
                            self.entities[moved_entity.id() as usize].index = old_idx;
                        }
                    }
                }
                EntityCommand::RemoveTag { entity, id } => {
                    let mut info = &mut self.entities[entity.id() as usize];
                    assert!(info.ver.get() == entity.ver());
                    if let Some(collection) = info.collection {
                        info.collection = tags.remove_tag(entity, id, collection);
                    }
                }
                EntityCommand::AddTag { entity, tag } => {
                    let mut info = &mut self.entities[entity.id() as usize];
                    assert!(info.ver.get() == entity.ver());
                    info.collection = Some(tags.add_tag(entity, info.collection, tag));
                }
            }
        }

        // Process to create
        for mut pack in self.to_create.drain(..) {
            // Move the components into their archetype
            let (archetype, begin) = pack.components.move_into(&pack.entities, archetypes);

            // Move tags into their storages if needed
            let collection = if let Some(mut tag_pack) = pack.tags {
                Some(tag_pack.move_into(&pack.entities, tags))
            } else {
                None
            };

            // Update the created entities archetypes
            for (i, entity) in pack.entities.iter().enumerate() {
                let mut info = &mut self.entities[entity.id() as usize];
                info.archetype = archetype;
                info.collection = collection;
                info.index = (begin + i) as u32;
            }
        }

        // Process to destroy
        for entity in self.to_destroy.drain(..) {
            let info = &mut self.entities[entity.id() as usize];

            // Remove tags if needed
            if let Some(id) = &info.collection {
                let collection = tags
                    .get_collection(*id)
                    .expect("Tag collection does not exist for entity");
                for id in collection.type_key.iter() {
                    let storage = tags.get_storage_id(*id).unwrap();
                    tags.get_storage_by_id(storage).remove(&[entity]);
                }
            }

            // Remove components and swap the moved entity if needed
            if let Some(moved_entity) =
                archetypes.remove_entity(info.archetype, info.index as usize)
            {
                self.entities[moved_entity.id() as usize].index = info.index;
            }

            // Add entity to the free list
            self.free.push(entity.id());
        }
    }

    /// Requests that entities be destroyed.
    ///
    /// # Note
    /// See the note in `create_with_tags` about postponing creation. Same thing applies for
    /// deletion.
    ///
    /// # Panics
    /// Panics if any of the entities are invalide handles.
    pub fn destroy(&mut self, entities: &[Entity]) {
        // Add all entities to the "to destroy" list and then update their versions. Updating the
        // version happens here so we can avoid "double deletion"
        for entity in entities {
            // Verify the ID
            let ind = entity.id() as usize;
            assert!(ind < self.entities.len() && self.entities[ind].ver.get() == entity.ver());

            // Add to list
            self.to_destroy.push(*entity);

            // Update version
            self.entities[ind].ver =
                NonZeroU32::new(self.entities[ind].ver.get() + 1).expect("Entity verion overflow");
        }
    }

    /// Requests that entities be created using a set of components. A slice of the new entities
    /// handles is returned.
    ///
    /// # Note
    /// An alias for `create_with_tags(components, ())`.
    #[inline]
    pub fn create<C: ComponentPack + 'static>(&mut self, components: C) -> &[Entity] {
        self.create_with_tags(components, ())
    }

    /// Requests that entities be created using a set of components and tags. A slice of the
    /// new entities handles is returned.
    ///
    /// # Note
    /// The entities created by this function are not actually initialized. This is to allow
    /// adding entities to a world while a query is operating on the same world. The consequence
    /// of this is that you cannot add or remove tags or components from the entities once they
    /// are created until the next call to `process` on `self` which happens at the beginning
    /// of dispatch. You can, however, request for the entities to be destroyed.
    #[inline]
    pub fn create_with_tags<C: ComponentPack + 'static, T: TagPack + 'static>(
        &mut self,
        components: C,
        tags: T,
    ) -> &[Entity] {
        self.create_with_tags_dyn(
            Box::new(components),
            if tags.len() == 0 {
                None
            } else {
                Some(Box::new(tags))
            },
        )
    }

    pub fn create_with_tags_dyn(
        &mut self,
        components: Box<dyn ComponentPack>,
        tags: Option<Box<dyn TagPack>>,
    ) -> &[Entity] {
        assert!(components.is_valid());
        if let Some(tags) = &tags {
            assert!(tags.is_valid());
            assert!(tags.len() == 0 || tags.len() == components.len());
        }

        // Create entity handles
        let entities = self.gen_entities(components.len());

        // Create a pack for later initialization
        self.to_create.push(EntityCreateInfo {
            components,
            tags,
            entities,
        });

        &self.to_create[self.to_create.len() - 1].entities
    }

    /// Add a component to an entity. If the entity already has this component type, it is replaced
    /// by the new component.
    #[inline]
    pub fn add_component<C: ComponentExt + 'static>(&self, entity: Entity, component: C) {
        let _ = self.sender.send(EntityCommand::AddComponent {
            entity,
            component: Box::new(component),
        });
    }

    /// Remove a component from an entity. No-op if the component doesn't exist on the entity.
    #[inline]
    pub fn remove_component<C: Component + 'static>(&self, entity: Entity) {
        let _ = self.sender.send(EntityCommand::RemoveComponent {
            entity,
            id: TypeId::of::<C>(),
        });
    }

    /// Add a tag to an entity. If the entity already has this tag type, it is replaced by the new
    /// tag.
    #[inline]
    pub fn add_tag(&self, entity: Entity, tag: impl Tag + 'static) {
        let _ = self.sender.send(EntityCommand::AddTag {
            entity,
            tag: Box::new(tag),
        });
    }

    /// Remove a tag from an entity. No-op if the tag doesn't exist on the entity.
    #[inline]
    pub fn remove_tag<T: Tag + 'static>(&self, entity: Entity) {
        let _ = self.sender.send(EntityCommand::RemoveTag {
            entity,
            id: TypeId::of::<T>(),
        });
    }

    /// Helper method to create entities.
    fn gen_entities(&mut self, count: usize) -> Vec<Entity> {
        let mut entities = Vec::with_capacity(count);
        for _ in 0..count {
            // Attempt to get a free entity
            if let Some(free) = self.free.pop() {
                // NOTE: We use the same entity version since it is updated during deletion.
                entities.push(Entity::new(free, self.entities[free as usize].ver));
            }
            // New entity otherwise
            else {
                let ver = NonZeroU32::new(1).unwrap();
                self.entities.push(EntityInfo {
                    ver,
                    archetype: ArchetypeId::default(),
                    index: 0,
                    collection: None,
                });
                entities.push(Entity::new((self.entities.len() - 1) as u32, ver));
            }
        }

        entities
    }
}

impl EntityCommands {
    #[inline]
    pub fn destroy(&mut self, entities: &[Entity]) {
        let _ = self.sender.send(EntityCommand::Destroy {
            entities: Vec::from(entities),
        });
    }

    #[inline]
    pub fn create<C: ComponentPack + 'static>(&mut self, components: C) {
        self.create_with_tags(components, ());
    }

    #[inline]
    pub fn create_with_tags<C: ComponentPack + 'static, T: TagPack + 'static>(
        &mut self,
        components: C,
        tags: T,
    ) {
        let _ = self.sender.send(EntityCommand::Create {
            components: Box::new(components),
            tags: if tags.is_empty() {
                None
            } else {
                Some(Box::new(tags))
            },
        });
    }

    #[inline]
    pub fn add_component<C: ComponentExt + 'static>(&self, entity: Entity, component: C) {
        let _ = self.sender.send(EntityCommand::AddComponent {
            entity,
            component: Box::new(component),
        });
    }

    #[inline]
    pub fn remove_component<C: Component + 'static>(&self, entity: Entity) {
        let _ = self.sender.send(EntityCommand::RemoveComponent {
            entity,
            id: TypeId::of::<C>(),
        });
    }

    #[inline]
    pub fn add_tag(&self, entity: Entity, tag: impl Tag + 'static) {
        let _ = self.sender.send(EntityCommand::AddTag {
            entity,
            tag: Box::new(tag),
        });
    }

    #[inline]
    pub fn remove_tag<T: Tag + 'static>(&self, entity: Entity) {
        let _ = self.sender.send(EntityCommand::RemoveTag {
            entity,
            id: TypeId::of::<T>(),
        });
    }
}
