#version 450 core

#define ARD_VERTEX_SHADER
#include "ard_std.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 NORMAL;
layout(location = 2) in vec2 UV0;

layout(location = 0) out vec4 SCREEN_POS;
layout(location = 1) out vec4 OUT_NORMAL;
layout(location = 2) out vec2 UV;

VsOut entry() {
    VsOut vs_out;
    mat4 model = get_model_matrix();
    vec4 frag_pos = model * vec4(POSITION.xyz, 1.0);
    gl_Position = camera.vp * frag_pos;
    SCREEN_POS = vec4(
        gl_Position.xy, 
        (gl_Position.w * camera.near_clip) / gl_Position.z, 
        gl_Position.w
    );
    OUT_NORMAL = vec4(normalize((get_normal_matrix() * vec4(normalize(NORMAL.xyz), 0.0)).xyz), 0.0);
    UV = UV0;
    vs_out.frag_pos = frag_pos.xyz;

    return vs_out;
}
ARD_ENTRY(entry)
