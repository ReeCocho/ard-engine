use std::time::{Duration, Instant};

use ard_ecs::prelude::*;
use glam::*;

const ENTITY_COUNT: usize = 100_000;
const ITER_COUNT: usize = 10_000;

#[derive(Component, Default, Copy, Clone)]
struct MatrixA(Mat4);

#[derive(Component, Default, Copy, Clone)]
struct MatrixB(Mat4);

#[derive(Component, Default, Copy, Clone)]
struct MatrixC(Mat4);

#[derive(Event, Copy, Clone)]
struct RunOnce;

fn main() {
    let bevy_time = run_bevy();
    let ard_time = run_ard();

    println!("Ard  : {} ns", ard_time.as_nanos());
    println!("Bevy : {} ns", bevy_time.as_nanos());
}

fn run_ard() -> Duration {
    let mut dur = Duration::default();

    let mut world = World::new();

    world.entities_mut().create((
        vec![MatrixA::default(); ENTITY_COUNT],
        vec![MatrixB::default(); ENTITY_COUNT],
        vec![MatrixC::default(); ENTITY_COUNT],
    ));

    world.process_entities();

    for _ in 0..ITER_COUNT {
        let start = Instant::now();
        let queries = QueryGenerator::new::<(Write<MatrixA>, Read<MatrixB>, Read<MatrixC>)>(
            world.tags(),
            world.archetypes(),
        );
        for (a, b, c) in queries.make::<(Write<MatrixA>, Read<MatrixB>, Read<MatrixC>)>() {
            a.0 = b.0 * c.0;
        }
        dur += Instant::now().duration_since(start);
    }

    dur.div_f64(ITER_COUNT as f64)
}

fn run_bevy() -> Duration {
    let mut dur = Duration::default();

    let mut world = bevy_ecs::world::World::new();
    world.spawn_batch(vec![
        (
            MatrixA::default(),
            MatrixB(Mat4::from_cols(
                Vec4::ONE,
                Vec4::ONE,
                -Vec4::ONE,
                2.0 * Vec4::ONE
            )),
            MatrixC(Mat4::from_cols(
                Vec4::ONE,
                Vec4::ONE,
                -Vec4::ONE,
                2.0 * Vec4::ONE
            ))
        );
        ENTITY_COUNT
    ]);

    for _ in 0..ITER_COUNT {
        let start = Instant::now();
        for (mut a, b, c) in world
            .query::<(&mut MatrixA, &MatrixB, &MatrixC)>()
            .iter_mut(&mut world)
        {
            a.0 = b.0 * c.0;
        }
        dur += Instant::now().duration_since(start);
    }

    dur.div_f64(ITER_COUNT as f64)
}
