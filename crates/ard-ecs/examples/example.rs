use ard_ecs::prelude::*;

// Components

#[derive(Component)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Component)]
struct Velocity {
    x: f32,
    y: f32,
}

// Events

#[derive(Event, Clone, Copy)]
struct PhysicsStep(f32);

// Systems

struct Physics {}

impl SystemState for Physics {
    type Data = (Write<Position>, Read<Velocity>);
    type Resources = ();
}

impl Into<System> for Physics {
    fn into(self) -> System {
        SystemBuilder::new(self).with_handler(Physics::step).build()
    }
}

impl Physics {
    fn step(&mut self, ctx: Context<Self>, event: PhysicsStep) {
        let delta = event.0;
        for (pos, vel) in ctx.queries.make::<(Write<Position>, Read<Velocity>)>() {
            pos.x += vel.x * delta;
            pos.y += vel.y * delta;
            println!("New Position: {} {}", pos.x, pos.y);
        }
    }
}

fn main() {
    let resources = Resources::new();

    let mut world = World::new();

    world.entities_mut().create((
        vec![Position { x: 0.0, y: 0.0 }],
        vec![Velocity { x: 1.0, y: -1.0 }],
    ));

    let mut dispatcher = Dispatcher::new();
    dispatcher.add_system(Physics {});
    dispatcher.submit(PhysicsStep(1.0 / 30.0));
    dispatcher.submit(PhysicsStep(1.0 / 30.0));
    dispatcher.submit(PhysicsStep(1.0 / 30.0));

    dispatcher.run(&mut world, &resources);
}
