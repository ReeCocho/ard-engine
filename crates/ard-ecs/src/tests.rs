use ard_ecs_derive::SystemState;

use crate::{prelude::*, prw_lock::PrwLock, resource::res::Res, system::commands::Commands};

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

fn handler<S: SystemState, E: Event>(_: &mut S, _: E, _: Commands, _: Queries<()>, res: Res<()>) {}

#[test]
fn prw_lock_test() {
    let lock = PrwLock::new(42, "");

    let handle1 = lock.read();
    assert_eq!(*handle1, 42);

    let handle2 = lock.read();
    assert_eq!(*handle2, 42);

    std::mem::drop(handle1);
    std::mem::drop(handle2);

    let mut handle3 = lock.write();
    assert_eq!(*handle3, 42);

    *handle3 += 27;

    assert_eq!(*handle3, 69);
}

/// PrwLock should panic if there are multiple writers.
#[test]
#[should_panic]
fn prw_lock_multiple_writers() {
    let lock = PrwLock::new(42, "");
    let mut _handle1 = lock.write();
    let mut _handle2 = lock.write();
}

/// PrwLock should panic if there is a reader and a writer.
#[test]
#[should_panic]
fn prw_lock_readers_and_writers() {
    let lock = PrwLock::new(42, "");
    let mut _handle1 = lock.read();
    let mut _handle2 = lock.write();
}

/// PrwLock should not panic if there are multiple readers.
#[test]
fn prw_lock_readers() {
    let lock = PrwLock::new(42, "");
    let mut _handle1 = lock.read();
    let mut _handle2 = lock.read();
}

/// PrwLock should not panic if there is a reader then a writer.
#[test]
fn prw_lock_reader_then_writer() {
    let lock = PrwLock::new(42, "");
    let handle1 = lock.read();
    std::mem::drop(handle1);
    let mut _handle2 = lock.write();
}

/// PrwLock should not panic if there is a writer then a reader.
#[test]
fn prw_lock_writer_then_reader() {
    let lock = PrwLock::new(42, "");
    let handle1 = lock.write();
    std::mem::drop(handle1);
    let mut _handle2 = lock.read();
}

/// Creating entities properly intitializes the component data.
#[test]
fn create_entities() {
    const ENTITY_COUNT: usize = 250;

    let mut world = World::new();

    let a = ComponentA { x: 1, y: 2 };

    let b = ComponentB { x: 3, y: 4 };

    let c = ComponentC { x: 5, y: 6 };

    let mut entities = vec![Entity::null(); ENTITY_COUNT];
    world.entities().commands().create(
        (
            vec![a; ENTITY_COUNT],
            vec![b; ENTITY_COUNT],
            vec![c; ENTITY_COUNT],
        ),
        &mut entities,
    );

    for entity in entities {
        assert_ne!(entity, Entity::null());
    }

    let mut entities = vec![Entity::null(); ENTITY_COUNT];
    world.entities().commands().create(
        (
            vec![a; ENTITY_COUNT],
            vec![b; ENTITY_COUNT],
            vec![c; ENTITY_COUNT],
        ),
        &mut entities,
    );

    for entity in entities {
        assert_ne!(entity, Entity::null());
    }

    world.process_entities();

    let gen = Queries::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>::new(
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

/// Destroying entities should remove them after processing.
#[test]
fn destroy_entities() {
    const ENTITY_COUNT: usize = 250;

    let mut world = World::new();

    let a = ComponentA { x: 1, y: 2 };

    let b = ComponentB { x: 3, y: 4 };

    let c = ComponentC { x: 5, y: 6 };

    let mut entities = vec![Entity::null(); ENTITY_COUNT];
    world.entities().commands().create(
        (
            vec![a; ENTITY_COUNT],
            vec![b; ENTITY_COUNT],
            vec![c; ENTITY_COUNT],
        ),
        &mut entities,
    );

    world.process_entities();
    world.entities().commands().destroy(&entities);
    world.process_entities();

    let gen = Queries::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>::new(
        world.tags(),
        world.archetypes(),
    );
    for (_, _, _) in gen.make::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>() {
        panic!();
    }
}

/// Creating entities properly initializes tag data.
#[test]
fn create_with_tags() {
    let mut world = World::new();

    let ac = ComponentA { x: 1, y: 2 };

    let at = TagA { x: 3, y: 4 };

    world
        .entities()
        .commands()
        .create_with_tags((vec![ac; 1],), (vec![at; 1],), &mut []);
    world.entities().commands().create((vec![ac; 1],), &mut []);
    world.process_entities();

    let gen = Queries::<(Entity, (Read<ComponentA>,), (Read<TagA>,))>::new(
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

/// Resources are optional.
#[test]
fn resources() {
    #[derive(SystemState)]
    struct SysA;

    #[derive(SystemState)]
    struct SysB;

    impl SysA {
        fn run(&mut self, _: RunOnce, _: Commands, _: Queries<()>, res: Res<(Read<ResourceA>,)>) {
            let resource = res.get();
            let resource = resource.0.as_ref().unwrap();
            assert_eq!(resource.x, 1);
            assert_eq!(resource.y, 2);
        }
    }

    impl Into<System> for SysA {
        fn into(self) -> System {
            SystemBuilder::new(self).with_handler(SysA::run).build()
        }
    }

    impl SysB {
        fn run(&mut self, _: RunOnce, _: Commands, _: Queries<()>, res: Res<(Read<ResourceB>,)>) {
            assert!(res.get().0.is_none());
        }
    }

    impl Into<System> for SysB {
        fn into(self) -> System {
            SystemBuilder::new(self).with_handler(SysB::run).build()
        }
    }

    let mut resources = Resources::new();
    resources.add(ResourceA { x: 1, y: 2 });

    let mut dispatcher = Dispatcher::builder()
        .add_system(SysA)
        .add_system(SysB)
        .build();

    let mut world = World::new();

    dispatcher.submit(RunOnce {});
    dispatcher.run(&mut world, &resources);
}

/// Removing components at runtime.
#[test]
fn remove_components() {
    const ENTITY_COUNT: usize = 250;

    let mut world = World::new();

    let a = ComponentA { x: 1, y: 2 };

    let b = ComponentB { x: 3, y: 4 };

    let c = ComponentC { x: 5, y: 6 };

    let mut entities = vec![Entity::null(); ENTITY_COUNT];
    world.entities().commands().create(
        (
            vec![a; ENTITY_COUNT],
            vec![b; ENTITY_COUNT],
            vec![c; ENTITY_COUNT],
        ),
        &mut entities,
    );

    world.process_entities();

    let gen = Queries::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>::new(
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

    let gen = Queries::<(
        Entity,
        (Read<ComponentA>, Read<ComponentB>, Read<ComponentC>),
    )>::new(world.tags(), world.archetypes());

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

/// Adding components at runtime.
#[test]
fn add_components() {
    let mut world = World::new();

    let a = ComponentA { x: 1, y: 2 };
    let b = ComponentB { x: 3, y: 4 };
    let c = ComponentC { x: 5, y: 6 };

    let mut entities = vec![Entity::null(); 4];
    world
        .entities()
        .commands()
        .create((vec![a; 4],), &mut entities);
    world.process_entities();

    world.entities().commands().add_component(entities[1], b);
    world.entities().commands().add_component(entities[2], c);
    world.entities().commands().add_component(entities[3], b);
    world.entities().commands().add_component(entities[3], c);

    world.process_entities();

    let gen = Queries::<(Read<ComponentA>, Read<ComponentB>, Read<ComponentC>)>::new(
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

/// Adding tags at runtime.
#[test]
fn add_tags() {
    let mut world = World::new();

    let ac = ComponentA { x: 1, y: 2 };
    let at = TagA { x: 2, y: 3 };
    let bt = TagB { x: 4, y: 5 };

    let mut entities = vec![Entity::null(); 4];
    world
        .entities()
        .commands()
        .create((vec![ac; 4],), &mut entities);
    world.process_entities();

    world.entities().commands().add_tag(entities[1], at);
    world.entities().commands().add_tag(entities[2], bt);
    world.entities().commands().add_tag(entities[3], at);
    world.entities().commands().add_tag(entities[3], bt);

    world.process_entities();

    let gen = Queries::<(Entity, (Read<ComponentA>,), (Read<TagA>, Read<TagB>))>::new(
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

/// Remove tags at runtime.
#[test]
fn remove_tags() {
    let mut world = World::new();

    let ac = ComponentA { x: 1, y: 2 };
    let at = TagA { x: 2, y: 3 };
    let bt = TagB { x: 4, y: 5 };

    let mut entities = vec![Entity::null(); 4];
    world.entities().commands().create_with_tags(
        (vec![ac; 4],),
        (vec![at; 4], vec![bt; 4]),
        &mut entities,
    );

    world.process_entities();

    world.entities().commands().remove_tag::<TagA>(entities[1]);
    world.entities().commands().remove_tag::<TagB>(entities[2]);
    world.entities().commands().remove_tag::<TagA>(entities[3]);
    world.entities().commands().remove_tag::<TagB>(entities[3]);

    world.process_entities();

    let gen = Queries::<(Entity, (Read<ComponentA>,), (Read<TagA>, Read<TagB>))>::new(
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

/// Runs systems in parallel.
#[test]
fn parallel_systems() {
    struct SystemExclusive;

    #[derive(SystemState)]
    struct SystemA;

    #[derive(SystemState)]
    struct SystemB;

    #[derive(SystemState)]
    struct SystemC;

    #[derive(SystemState)]
    struct SystemAB;

    #[derive(SystemState)]
    struct SystemBC;

    #[derive(SystemState)]
    struct SystemABC;

    impl SystemState for SystemExclusive {
        const MAIN_THREAD: bool = true;

        const DEBUG_NAME: &'static str = "SystemExclusive";
    }

    let mut dispatcher = Dispatcher::builder()
        .add_system(
            SystemBuilder::new(SystemExclusive)
                .with_handler(handler::<SystemExclusive, RunOnce>)
                .build(),
        )
        .add_system(
            SystemBuilder::new(SystemA)
                .with_handler(handler::<SystemA, RunOnce>)
                .build(),
        )
        .add_system(
            SystemBuilder::new(SystemB)
                .with_handler(handler::<SystemB, RunOnce>)
                .build(),
        )
        .add_system(
            SystemBuilder::new(SystemC)
                .with_handler(handler::<SystemC, RunOnce>)
                .build(),
        )
        .add_system(
            SystemBuilder::new(SystemAB)
                .with_handler(handler::<SystemAB, RunOnce>)
                .build(),
        )
        .add_system(
            SystemBuilder::new(SystemBC)
                .with_handler(handler::<SystemBC, RunOnce>)
                .build(),
        )
        .add_system(
            SystemBuilder::new(SystemABC)
                .with_handler(handler::<SystemABC, RunOnce>)
                .build(),
        )
        .build();

    let mut world = World::default();
    let resources = Resources::default();

    dispatcher.run(&mut world, &resources);
}

/// Dispatcher should panic when circular dependencies are detected.
#[test]
#[should_panic]
fn circular_dependency() {
    #[derive(SystemState)]
    struct SystemA;

    #[derive(SystemState)]
    struct SystemB;

    #[derive(SystemState)]
    struct SystemC;

    let mut dispatcher = Dispatcher::builder()
        .add_system(
            SystemBuilder::new(SystemA)
                .with_handler(handler::<SystemA, RunOnce>)
                .run_before::<RunOnce, SystemB>()
                .build(),
        )
        .add_system(
            SystemBuilder::new(SystemB)
                .with_handler(handler::<SystemB, RunOnce>)
                .run_before::<RunOnce, SystemC>()
                .build(),
        )
        .add_system(
            SystemBuilder::new(SystemC)
                .with_handler(handler::<SystemC, RunOnce>)
                .run_before::<RunOnce, SystemA>()
                .build(),
        )
        .build();

    let mut world = World::default();
    let resources = Resources::default();

    dispatcher.run(&mut world, &resources);
}
