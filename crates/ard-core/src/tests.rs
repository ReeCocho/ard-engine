use crate::prelude::*;
use ard_ecs::{prelude::*, resource::res::Res, system::commands::Commands};

#[test]
fn game_loop() {
    const TICK_COUNT: usize = 69;

    #[derive(Default)]
    struct Ticker {
        start_count: usize,
        tick_count: usize,
        post_tick_count: usize,
    }

    impl SystemState for Ticker {}

    impl Ticker {
        fn start(&mut self, _: Start, _: Commands, _: Queries<()>, _: Res<()>) {
            assert_eq!(self.start_count, 0);
            self.start_count += 1;
        }

        fn tick(&mut self, _: Tick, _: Commands, _: Queries<()>, _: Res<()>) {
            self.tick_count += 1;
        }

        fn post_tick(&mut self, _: PostTick, commands: Commands, _: Queries<()>, _: Res<()>) {
            self.post_tick_count += 1;
            if self.post_tick_count == TICK_COUNT {
                assert_eq!(self.tick_count, TICK_COUNT);
                assert_eq!(self.start_count, 1);
                commands.events.submit(Stop);
            }
        }
    }

    impl Into<System> for Ticker {
        fn into(self) -> System {
            SystemBuilder::new(Ticker::default())
                .with_handler(Ticker::start)
                .with_handler(Ticker::tick)
                .with_handler(Ticker::post_tick)
                .build()
        }
    }

    AppBuilder::new()
        .add_plugin(ArdCorePlugin)
        .add_system(Ticker::default())
        .run();
}
