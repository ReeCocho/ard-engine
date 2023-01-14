use ard_pal::prelude::*;

pub struct MeshBuffers {
    pub vertex: Buffer,
    pub vertex_staging: Buffer,
    pub index: Buffer,
    pub index_staging: Buffer,
}

pub fn create_triangle(ctx: &Context) -> MeshBuffers {
    const INDICES: &'static [u16] = &[0, 1, 2];
    const VERTICES: &'static [f32] = &[
        -1.0, -1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, // First
        1.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, // Second
        0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, // Third
    ];

    MeshBuffers {
        vertex: Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (VERTICES.len() * std::mem::size_of::<f32>()) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("triangle_vertex_buffer")),
            },
        )
        .unwrap(),
        vertex_staging: Buffer::new_staging(
            ctx.clone(),
            Some(String::from("triangle_vertex_staging")),
            bytemuck::cast_slice(&VERTICES),
        )
        .unwrap(),
        index: Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (INDICES.len() * std::mem::size_of::<u16>()) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::INDEX_BUFFER | BufferUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("triangle_index_buffer")),
            },
        )
        .unwrap(),
        index_staging: Buffer::new_staging(
            ctx.clone(),
            Some(String::from("triangle_index_staging")),
            bytemuck::cast_slice(&INDICES),
        )
        .unwrap(),
    }
}

#[allow(dead_code)]
pub fn create_cube(ctx: &Context) -> MeshBuffers {
    const INDICES: &'static [u16] = &[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
    ];
    const VERTICES: &'static [f32] = &[
        -0.5, -0.5, -0.5, 1.0, 0.0, 0.0, 0.5, -0.5, -0.5, 1.0, 1.0, 0.0, 0.5, 0.5, -0.5, 1.0, 1.0,
        1.0, 0.5, 0.5, -0.5, 1.0, 1.0, 1.0, -0.5, 0.5, -0.5, 1.0, 0.0, 1.0, -0.5, -0.5, -0.5, 1.0,
        0.0, 0.0, -0.5, -0.5, 0.5, 1.0, 0.0, 0.0, 0.5, -0.5, 0.5, 1.0, 1.0, 0.0, 0.5, 0.5, 0.5,
        1.0, 1.0, 1.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0, -0.5, 0.5, 0.5, 1.0, 0.0, 1.0, -0.5, -0.5,
        0.5, 1.0, 0.0, 0.0, -0.5, 0.5, 0.5, 1.0, 1.0, 0.0, -0.5, 0.5, -0.5, 1.0, 1.0, 1.0, -0.5,
        -0.5, -0.5, 1.0, 0.0, 1.0, -0.5, -0.5, -0.5, 1.0, 0.0, 1.0, -0.5, -0.5, 0.5, 1.0, 0.0, 0.0,
        -0.5, 0.5, 0.5, 1.0, 1.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 0.0, 0.5, 0.5, -0.5, 1.0, 1.0, 1.0,
        0.5, -0.5, -0.5, 1.0, 0.0, 1.0, 0.5, -0.5, -0.5, 1.0, 0.0, 1.0, 0.5, -0.5, 0.5, 1.0, 0.0,
        0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 0.0, -0.5, -0.5, -0.5, 1.0, 0.0, 1.0, 0.5, -0.5, -0.5, 1.0,
        1.0, 1.0, 0.5, -0.5, 0.5, 1.0, 1.0, 0.0, 0.5, -0.5, 0.5, 1.0, 1.0, 0.0, -0.5, -0.5, 0.5,
        1.0, 0.0, 0.0, -0.5, -0.5, -0.5, 1.0, 0.0, 1.0, -0.5, 0.5, -0.5, 1.0, 0.0, 1.0, 0.5, 0.5,
        -0.5, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5, 1.0, 1.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 0.0, -0.5, 0.5,
        0.5, 1.0, 0.0, 0.0, -0.5, 0.5, -0.5, 1.0, 0.0, 1.0,
    ];

    MeshBuffers {
        vertex: Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (VERTICES.len() * std::mem::size_of::<f32>()) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::VERTEX_BUFFER | BufferUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("cube_vertex_buffer")),
            },
        )
        .unwrap(),
        vertex_staging: Buffer::new_staging(
            ctx.clone(),
            Some(String::from("cube_vertex_staging")),
            bytemuck::cast_slice(&VERTICES),
        )
        .unwrap(),
        index: Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (INDICES.len() * std::mem::size_of::<u16>()) as u64,
                array_elements: 1,
                buffer_usage: BufferUsage::INDEX_BUFFER | BufferUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("cube_index_buffer")),
            },
        )
        .unwrap(),
        index_staging: Buffer::new_staging(
            ctx.clone(),
            Some(String::from("cube_index_staging")),
            bytemuck::cast_slice(&INDICES),
        )
        .unwrap(),
    }
}
