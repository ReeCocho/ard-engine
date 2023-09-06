use ard_pal::prelude::Texture;

pub struct RenderGraphTextures {
    textures: Vec<RenderGraphTexture>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderGraphTextureId(usize);

struct RenderGraphTexture {
    texture: Texture,
    discarded: bool,
}

impl From<RenderGraphTextureId> for usize {
    fn from(value: RenderGraphTextureId) -> Self {
        value.0
    }
}