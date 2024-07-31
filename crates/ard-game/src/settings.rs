use ard_ecs::prelude::*;
use ard_pal::prelude::MultiSamples;
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Resource)]
pub struct GameSettings {
    pub smaa: bool,
    pub msaa: MultiSamples,
    pub target_frame_rate: Option<usize>,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            smaa: true,
            msaa: MultiSamples::Count1,
            target_frame_rate: Some(60),
        }
    }
}

impl GameSettings {
    pub fn load() -> Option<Self> {
        let file = std::fs::File::open("./settings.ron").ok()?;
        let reader = std::io::BufReader::new(file);
        ron::de::from_reader::<_, GameSettings>(reader).ok()
    }

    pub fn save(&self) {
        let file = match std::fs::File::create("./settings.ron") {
            Ok(file) => file,
            Err(_) => return,
        };
        let writer = std::io::BufWriter::new(file);
        let _ = ron::ser::to_writer_pretty(writer, self, PrettyConfig::default());
    }
}
