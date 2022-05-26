// Compile with:
// glslc triangle.frag -o triangle.frag.spv
#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 VERT_COLOR;

void main() {
    FRAGMENT_COLOR = VERT_COLOR;
}