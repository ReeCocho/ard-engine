#version 450

#define ARD_FRAGMENT_SHADER
#include "ard_std.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 VERT_COLOR;

void entry() {
    FRAGMENT_COLOR = VERT_COLOR;
}
ARD_ENTRY(entry)
