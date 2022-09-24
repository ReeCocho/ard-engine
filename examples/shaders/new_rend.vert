#version 450 core

#define ARD_VERTEX_SHADER
#include "ard_std.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 COLOR;

layout(location = 0) out vec4 VERT_COLOR;

VsOut entry() {
    VsOut vs_out;
    mat4 model = get_model_matrix();
    vec4 frag_pos = model * POSITION;
    gl_Position = camera.vp * frag_pos;
    VERT_COLOR = COLOR;
    vs_out.frag_pos = frag_pos.xyz;

    return vs_out;
}
ARD_ENTRY(entry)
