// Compile with:
// glslc triangle.vert -o triangle.vert.spv
#version 450 core

#define ARD_VERTEX_SHADER
#include "user_shaders.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 COLOR;

layout(location = 1) out vec4 VERT_COLOR;

VsOut entry() {
    VsOut vs_out;
    vs_out.frag_pos = (MODEL_MATRIX * vec4(POSITION.xyz, 1.0)).xyz;
    gl_Position = camera.vp * vec4(vs_out.frag_pos, 1.0);
    VERT_COLOR = COLOR;
    return vs_out;
}

ARD_ENTRY(entry)