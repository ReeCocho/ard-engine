// Compile with:
// glslc triangle.frag -o triangle.frag.spv
#version 450

#define ARD_FRAGMENT_SHADER
#include "user_shaders.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 1) in vec4 VERT_COLOR;

void entry() {
    FRAGMENT_COLOR = VERT_COLOR;
}

ARD_ENTRY(entry)