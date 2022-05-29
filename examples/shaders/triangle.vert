// Compile with:
// glslc triangle.vert -o triangle.vert.spv
#version 450 core

#define ARD_VERTEX_SHADER
#include "user_shaders.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 COLOR;

layout(location = 1) out vec4 VERT_COLOR;

void entry() {
    gl_Position = camera.vp * get_model_matrix() * vec4(POSITION.xyz, 1.0);
    VERT_COLOR = COLOR;
}

ARD_ENTRY(entry)