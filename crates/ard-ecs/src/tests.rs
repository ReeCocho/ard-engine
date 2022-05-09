use crate::prelude::*;

#[derive(Component, Debug, Default, Copy, Clone, PartialEq, Eq)]
struct ComponentA {
    x: u32,
    y: u32,
}

#[derive(Component, Debug, Default, Copy, Clone, PartialEq, Eq)]
struct ComponentB {
    x: u32,
    y: u32,
}

#[derive(Component, Debug, Default, Copy, Clone, PartialEq, Eq)]
struct ComponentC {
    x: u32,
    y: u32,
}

#[derive(Tag, Debug, Default, Copy, Clone, PartialEq, Eq)]
#[storage(CommonStorage)]
struct TagA {
    x: u32,
    y: u32,
}

#[derive(Tag, Debug, Default, Copy, Clone, PartialEq, Eq)]
#[storage(UncommonStorage)]
struct TagB {
    x: u32,
    y: u32,
}

#[derive(Resource, Debug, Default, Copy, Clone, PartialEq, Eq)]
struct ResourceA {
    x: u32,
    y: u32,
}

#[derive(Resource, Debug, Default, Copy, Clone, PartialEq, Eq)]
struct ResourceB {
    x: u32,
    y: u32,
}

#[derive(Event, Debug, Clone)]
struct RunOnce;

#[test]
fn create_entities() {
    const ENTITY_COUNT: usize = 250;

    let mut world = World::new();

    let a = ComponentA { x: 1, y: 2 };

    let b = ComponentB { x: 3, y: 4 };

    let c = ComponentC { x: 5, y: 6 };

    let entities_len = world
        .entities_mut()
        .create((
            vec![a; ENTITY_COUNT],
            vec![b; ENTITY_COUNT],
            vec![c; ENTITY_COUNT],
        ))
        .len();
    assert_eq!(entities_len, ENTITY_COUNT);

    let entities_len = world
        .entities_mut()
        .create((
            vec![a; ENTITY_COUNT],
            vec![b; ENTITY_COUNT],
            vec![c; ENTITY_COUNT],
        ))
        .len();
    assert_eq!(entities_len, ENTITY_COUNT);

    world.process_entities();

    let gen = QueryGenerator::new::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>(
        world.tags(),
        world.archetypes(),
    );
    let mut count = 0;
    for (qa, qb, qc) in gen.make::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>() {
        assert_eq!(*qa, a);
        assert_eq!(*qb, b);
        assert_eq!(*qc, c);
        count += 1;
    }
    assert_eq!(count, ENTITY_COUNT * 2);
}

#[test]
fn destroy_entities() {
    const ENTITY_COUNT: usize = 250;

    let mut world = World::new();

    let a = ComponentA { x: 1, y: 2 };

    let b = ComponentB { x: 3, y: 4 };

    let c = ComponentC { x: 5, y: 6 };

    let entities: Vec<Entity> = world
        .entities_mut()
        .create((
            vec![a; ENTITY_COUNT],
            vec![b; ENTITY_COUNT],
            vec![c; ENTITY_COUNT],
        ))
        .into();
    world.process_entities();
    world.entities_mut().destroy(&entities);
    world.process_entities();

    let gen = QueryGenerator::new::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>(
        world.tags(),
        world.archetypes(),
    );
    for (_, _, _) in gen.make::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>() {
        panic!();
    }
}

#[test]
fn create_with_tags() {
    let mut world = World::new();

    let ac = ComponentA { x: 1, y: 2 };

    let at = TagA { x: 3, y: 4 };

    world
        .entities_mut()
        .create_with_tags((vec![ac; 1],), (vec![at; 1],));
    world.entities_mut().create((vec![ac; 1],));
    world.process_entities();

    let gen = QueryGenerator::new::<(Entity, (Read<ComponentA>,), (Read<TagA>,))>(
        world.tags(),
        world.archetypes(),
    );
    let mut has_tag = false;
    let mut no_tag = false;
    for (_, _, (tag,)) in gen.make::<(Entity, (Read<ComponentA>,), (Read<TagA>,))>() {
        if let Some(tag) = tag {
            assert_eq!(tag.x, 3);
            assert_eq!(tag.y, 4);
            has_tag = true;
        }

        if tag.is_none() {
            no_tag = true;
        }
    }

    assert!(has_tag);
    assert!(no_tag);
}

#[test]
fn resources() {
    struct SysA;
    struct SysB;

    impl SysA {
        fn run(&mut self, ctx: Context<Self>, _: RunOnce) {
            let resource = ctx.resources.0.as_ref().unwrap();
            assert_eq!(resource.x, 1);
            assert_eq!(resource.y, 2);
        }
    }

    impl SystemState for SysA {
        type Data = ();
        type Resources = (Read<ResourceA>,);
    }

    impl Into<System> for SysA {
        fn into(self) -> System {
            SystemBuilder::new(self).with_handler(SysA::run).build()
        }
    }

    impl SysB {
        fn run(&mut self, ctx: Context<Self>, _: RunOnce) {
            assert!(ctx.resources.0.is_none());
        }
    }

    impl SystemState for SysB {
        type Data = ();
        type Resources = (Read<ResourceB>,);
    }

    impl Into<System> for SysB {
        fn into(self) -> System {
            SystemBuilder::new(self).with_handler(SysB::run).build()
        }
    }

    let mut resources = Resources::new();
    resources.add(ResourceA { x: 1, y: 2 });

    let mut dispatcher = Dispatcher::new();
    dispatcher.add_system(SysA {});
    dispatcher.add_system(SysB {});

    let mut world = World::new();

    dispatcher.submit(RunOnce {});
    dispatcher.run(&mut world, &resources);
}

#[test]
fn remove_components() {
    const ENTITY_COUNT: usize = 250;

    let mut world = World::new();

    let a = ComponentA { x: 1, y: 2 };

    let b = ComponentB { x: 3, y: 4 };

    let c = ComponentC { x: 5, y: 6 };

    let entities: Vec<Entity> = world
        .entities_mut()
        .create((
            vec![a; ENTITY_COUNT],
            vec![b; ENTITY_COUNT],
            vec![c; ENTITY_COUNT],
        ))
        .into();
    world.process_entities();

    let gen = QueryGenerator::new::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>(
        world.tags(),
        world.archetypes(),
    );

    let mut count = 0;
    for (_, _, _) in gen.make::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>() {
        count += 1;
    }
    assert_eq!(count, ENTITY_COUNT);
    std::mem::drop(gen);

    world
        .entities()
        .commands()
        .remove_component::<ComponentA>(entities[0]);
    world
        .entities()
        .commands()
        .remove_component::<ComponentB>(entities[1]);
    world
        .entities()
        .commands()
        .remove_component::<ComponentA>(entities[69]);
    world
        .entities()
        .commands()
        .remove_component::<ComponentC>(entities[ENTITY_COUNT - 1]);
    world
        .entities()
        .commands()
        .remove_component::<ComponentC>(entities[ENTITY_COUNT - 2]);
    world.process_entities();

    let gen = QueryGenerator::new::<(
        Entity,
        (Read<ComponentA>, Read<ComponentB>, Read<ComponentC>),
    )>(world.tags(), world.archetypes());

    let mut count = 0;
    let mut seen = std::collections::HashSet::<Entity>::default();

    for (entity, (_, _, _)) in gen.make::<(
        Entity,
        (Read<ComponentA>, Read<ComponentB>, Read<ComponentC>),
    )>() {
        assert!(seen.insert(entity));
        assert_ne!(entity.id(), 0);
        assert_ne!(entity.id(), 1);
        assert_ne!(entity.id(), 69);
        assert_ne!(entity.id(), ENTITY_COUNT as u32 - 2);
        assert_ne!(entity.id(), ENTITY_COUNT as u32 - 1);
        count += 1;
    }
    assert_eq!(count, ENTITY_COUNT - 5);
}

#[test]
fn add_components() {
    let mut world = World::new();

    let a = ComponentA { x: 1, y: 2 };
    let b = ComponentB { x: 3, y: 4 };
    let c = ComponentC { x: 5, y: 6 };

    let entities: Vec<Entity> = world.entities_mut().create((vec![a; 4],)).into();
    world.process_entities();

    world.entities().add_component(entities[1], b);
    world.entities().add_component(entities[2], c);
    world.entities().add_component(entities[3], b);
    world.entities().add_component(entities[3], c);

    world.process_entities();

    let gen = QueryGenerator::new::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>(
        world.tags(),
        world.archetypes(),
    );

    let mut count = 0;
    for _ in gen.make::<(Read<ComponentA>,)>() {
        count += 1;
    }
    assert_eq!(count, 4);

    let mut count = 0;
    for _ in gen.make::<(Read<ComponentB>,)>() {
        count += 1;
    }
    assert_eq!(count, 2);

    let mut count = 0;
    for _ in gen.make::<(Read<ComponentC>,)>() {
        count += 1;
    }
    assert_eq!(count, 2);

    let mut count = 0;
    for _ in gen.make::<(Read<ComponentA>, Read<ComponentB>)>() {
        count += 1;
    }
    assert_eq!(count, 2);

    let mut count = 0;
    for _ in gen.make::<(Read<ComponentA>, Read<ComponentC>)>() {
        count += 1;
    }
    assert_eq!(count, 2);

    let mut count = 0;
    for _ in gen.make::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>() {
        count += 1;
    }
    assert_eq!(count, 1);
}

#[test]
fn add_tags() {
    let mut world = World::new();

    let ac = ComponentA { x: 1, y: 2 };
    let at = TagA { x: 2, y: 3 };
    let bt = TagB { x: 4, y: 5 };

    let entities: Vec<Entity> = world.entities_mut().create((vec![ac; 4],)).into();
    world.process_entities();

    world.entities().add_tag(entities[1], at);
    world.entities().add_tag(entities[2], bt);
    world.entities().add_tag(entities[3], at);
    world.entities().add_tag(entities[3], bt);

    world.process_entities();

    let gen = QueryGenerator::new::<(Entity, (Read<ComponentA>,), (Read<TagA>, Read<TagB>))>(
        world.tags(),
        world.archetypes(),
    );

    let mut has_a = 0;
    let mut has_b = 0;
    let mut has_both = 0;
    let mut has_none = 0;
    let mut count = 0;
    for (_, _, (a, b)) in gen.make::<(Entity, (Read<ComponentA>,), (Read<TagA>, Read<TagB>))>() {
        count += 1;

        if a.is_some() {
            has_a += 1;
        }

        if b.is_some() {
            has_b += 1;
        }

        if a.is_some() && b.is_some() {
            has_both += 1;
        }

        if a.is_none() && b.is_none() {
            has_none += 1;
        }
    }
    assert_eq!(count, 4);
    assert_eq!(has_a, 2);
    assert_eq!(has_b, 2);
    assert_eq!(has_both, 1);
    assert_eq!(has_none, 1);
}

#[test]
fn remove_tags() {
    let mut world = World::new();

    let ac = ComponentA { x: 1, y: 2 };
    let at = TagA { x: 2, y: 3 };
    let bt = TagB { x: 4, y: 5 };

    let entities: Vec<Entity> = world
        .entities_mut()
        .create_with_tags((vec![ac; 4],), (vec![at; 4], vec![bt; 4]))
        .into();
    world.process_entities();

    world.entities().remove_tag::<TagA>(entities[1]);
    world.entities().remove_tag::<TagB>(entities[2]);
    world.entities().remove_tag::<TagA>(entities[3]);
    world.entities().remove_tag::<TagB>(entities[3]);

    world.process_entities();

    let gen = QueryGenerator::new::<(Entity, (Read<ComponentA>,), (Read<TagA>, Read<TagB>))>(
        world.tags(),
        world.archetypes(),
    );

    let mut has_a = 0;
    let mut has_b = 0;
    let mut has_both = 0;
    let mut has_none = 0;
    let mut count = 0;
    for (_, _, (a, b)) in gen.make::<(Entity, (Read<ComponentA>,), (Read<TagA>, Read<TagB>))>() {
        count += 1;

        if a.is_some() {
            has_a += 1;
        }

        if b.is_some() {
            has_b += 1;
        }

        if a.is_some() && b.is_some() {
            has_both += 1;
        }

        if a.is_none() && b.is_none() {
            has_none += 1;
        }
    }
    assert_eq!(count, 4);
    assert_eq!(has_a, 2);
    assert_eq!(has_b, 2);
    assert_eq!(has_both, 1);
    assert_eq!(has_none, 1);
}

#[test]
fn parallel_systems() {
    struct SystemExclusive;
    struct SystemA;
    struct SystemB;
    struct SystemC;
    struct SystemAB;
    struct SystemBC;
    struct SystemABC;

    impl SystemState for SystemExclusive {
        const EXCLUSIVE: bool = true;
        type Data = ();
        type Resources = ();
    }

    impl SystemState for SystemA {
        type Data = (Write<ComponentA>,);
        type Resources = ();
    }

    impl SystemState for SystemB {
        type Data = (Write<ComponentB>,);
        type Resources = ();
    }

    impl SystemState for SystemC {
        type Data = (Write<ComponentC>,);
        type Resources = ();
    }

    impl SystemState for SystemAB {
        type Data = (Write<ComponentA>, Write<ComponentB>);
        type Resources = ();
    }

    impl SystemState for SystemBC {
        type Data = (Write<ComponentB>, Write<ComponentC>);
        type Resources = ();
    }

    impl SystemState for SystemABC {
        type Data = (Write<ComponentA>, Write<ComponentB>, Write<ComponentC>);
        type Resources = ();
    }

    let mut dispatcher = Dispatcher::new();
    dispatcher.add_system(SystemBuilder::new(SystemExclusive).build());
    dispatcher.add_system(SystemBuilder::new(SystemA).build());
    dispatcher.add_system(SystemBuilder::new(SystemB).build());
    dispatcher.add_system(SystemBuilder::new(SystemC).build());
    dispatcher.add_system(SystemBuilder::new(SystemAB).build());
    dispatcher.add_system(SystemBuilder::new(SystemBC).build());
    dispatcher.add_system(SystemBuilder::new(SystemABC).build());

    let stages = dispatcher.stages();
    assert_eq!(stages.len(), 4);
    assert!(stages[0].main().is_some());
    assert_eq!(stages[0].parallel().len(), 3);
    assert_eq!(stages[1].parallel().len(), 1);
    assert_eq!(stages[2].parallel().len(), 1);
    assert_eq!(stages[3].parallel().len(), 1);
}
