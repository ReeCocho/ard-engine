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

layout(location = 0) out vec4 VERT_COLOR;
layout(location = 1) out vec4 SCREEN_POS;

VsOut entry() {
    VsOut vs_out;
    mat4 model = get_model_matrix();
    vec4 frag_pos = model * POSITION;
    gl_Position = camera.vp * frag_pos;
    SCREEN_POS = vec4(
        gl_Position.xy, 
        (gl_Position.w * camera.near_clip) / gl_Position.z, 
        gl_Position.w
    );
    VERT_COLOR = COLOR;
    vs_out.frag_pos = frag_pos.xyz;

    return vs_out;
}
ARD_ENTRY(entry)
