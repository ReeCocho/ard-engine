#version 450 core
#extension GL_EXT_debug_printf : enable

#define ARD_FRAGMENT_SHADER
#include "ard_std.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

void entry() {
    FRAGMENT_COLOR = vec4(1.0);
}
ARD_ENTRY(entry)
