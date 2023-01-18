#version 450 core

#define ARD_VERTEX_SHADER
#include "ard_std.glsl"

layout(location = 0) in vec4 POSITION;

layout(location = 0) out vec4 VPOS;

VsOut entry() {
    VsOut vs_out;
    mat4 model = MODEL_MATRIX;
    vec4 frag_pos = model * vec4(POSITION.xyz, 1.0);
    gl_Position = camera.vp * frag_pos;
    VPOS = gl_Position;
    vs_out.frag_pos = frag_pos.xyz;
    return vs_out;
}
ARD_ENTRY(entry)
