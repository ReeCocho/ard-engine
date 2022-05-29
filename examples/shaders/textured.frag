// Compile with:
// glslc textured.frag -o textured.frag.spv
#version 450

#define ARD_FRAGMENT_SHADER
#include "user_shaders.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 1) in vec2 UV0;

void entry() {
    FRAGMENT_COLOR = sample_texture(0, UV0);
}

ARD_ENTRY(entry)