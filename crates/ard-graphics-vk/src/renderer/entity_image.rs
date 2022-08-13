use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;

use ash::vk;

use crate::VkBackend;

#[derive(Resource)]
pub struct EntityImage {
    pub(crate) canvas_size: vk::Extent2D,
    pub(crate) canvas: Vec<Entity>,
}

impl Default for EntityImage {
    fn default() -> Self {
        Self {
            canvas_size: vk::Extent2D {
                width: 1,
                height: 1,
            },
            canvas: vec![Entity::null(); 1],
        }
    }
}

impl EntityImageApi<VkBackend> for EntityImage {
    #[inline]
    fn sample(&self, uv: ard_math::Vec2) -> Option<Entity> {
        let x = (uv.x * (self.canvas_size.width - 1) as f32) as u32;
        let y = (uv.y * (self.canvas_size.height - 1) as f32) as u32;
        let idx = (y * self.canvas_size.width) + x;

        let entity = self.canvas[idx as usize];

        if entity == Entity::null() {
            None
        } else {
            Some(entity)
        }
    }
}

impl EntityImage {
    #[inline]
    pub(crate) fn canvas_size(&self) -> vk::Extent2D {
        self.canvas_size
    }

    #[inline]
    pub(crate) fn resize(&mut self, mut size: vk::Extent2D) {
        // Take quarter resolution always
        size.width = (size.width / 4).max(1);
        size.height = (size.height / 4).max(1);

        self.canvas_size = size;
        self.canvas.clear();
        self.canvas
            .resize(size.width as usize * size.height as usize, Entity::null());
    }
}
