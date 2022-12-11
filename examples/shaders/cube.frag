#version 450 core
#extension GL_EXT_debug_printf : enable


#define ARD_FRAGMENT_SHADER
#include "ard_std.glsl"

layout(location = 0) out vec4 OUT_COLOR;

layout(location = 0) in vec3 COLOR;

void main() {
    OUT_COLOR = vec4(COLOR, 1.0);
}