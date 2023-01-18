#version 450 core

#define ARD_VERTEX_SHADER
#include "ard_std.glsl"

layout(location = 0) in vec4 POSITION;

VsOut entry() {
    VsOut vs_out;
    mat4 model = MODEL_MATRIX;
    vec4 frag_pos = model * POSITION;
    gl_Position = camera.vp * frag_pos;
    vs_out.frag_pos = frag_pos.xyz;
    return vs_out;
}
ARD_ENTRY(entry)
