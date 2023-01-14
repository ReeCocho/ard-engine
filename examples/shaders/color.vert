#version 450 core

#define ARD_VERTEX_SHADER
#include "ard_std.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 NORMAL;
layout(location = 2) in vec4 TANGENT;
layout(location = 3) in vec4 COLOR;
layout(location = 4) in vec2 UV0;
layout(location = 5) in vec2 UV1;
layout(location = 6) in vec2 UV2;
layout(location = 7) in vec2 UV3;

VsOut entry() {
    VsOut vs_out;
    mat4 model = MODEL_MATRIX;
    vec4 frag_pos = model * POSITION;
    gl_Position = camera.vp * frag_pos;
    vs_out.frag_pos = frag_pos.xyz;
    return vs_out;
}
ARD_ENTRY(entry)
