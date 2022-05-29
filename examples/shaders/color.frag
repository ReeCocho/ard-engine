// Compile with:
// glslc triangle.frag -o triangle.frag.spv
#version 450

struct Material {
    vec4 color;
};

#define ARD_FRAGMENT_SHADER
#define ARD_MATERIAL Material
#include "user_shaders.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 1) in vec4 VERT_COLOR;


void entry() {
    FRAGMENT_COLOR = get_material_data().color;
}

ARD_ENTRY(entry)