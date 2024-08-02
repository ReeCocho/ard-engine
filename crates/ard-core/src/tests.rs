use crate::prelude::*;
use ard_ecs::{prelude::*, resource::res::Res, system::commands::Commands};

#[test]
fn game_loop() {
    const TICK_COUNT: usize = 69;

    #[derive(SystemState, Default)]
    struct Ticker {
        start_count: usize,
        tick_count: usize,
    }

    impl Ticker {
        fn start(&mut self, _: Start, _: Commands, _: Queries<()>, _: Res<()>) {
            assert_eq!(self.start_count, 0);
            self.start_count += 1;
        }

        fn tick(&mut self, _: Tick, commands: Commands, _: Queries<()>, _: Res<()>) {
            self.tick_count += 1;
            if self.tick_count == TICK_COUNT {
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
                .build()
        }
    }

    AppBuilder::new(ard_log::LevelFilter::Off)
        .add_plugin(ArdCorePlugin)
        .add_system(Ticker::default())
        .run();
}
