// Compile with:
// glslc textured.vert -o textured.vert.spv
#version 450 core

#define ARD_VERTEX_SHADER
#include "user_shaders.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec2 UV0;

layout(location = 1) out vec2 OUT_UV0;

void entry() {
    OUT_UV0 = UV0;
    gl_Position = camera.vp * get_model_matrix() * vec4(POSITION.xyz, 1.0);
}

ARD_ENTRY(entry)