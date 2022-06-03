#version 450 core

#define ARD_VERTEX_SHADER
#include "user_shaders.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 NORMAL;
layout(location = 2) in vec2 UV0;

layout(location = 0) out vec4 SCREEN_POS;
layout(location = 1) out vec3 OUT_NORMAL;
layout(location = 2) out vec2 OUT_UV0;

VsOut entry() {
    VsOut vs_out;

    mat4 model = get_model_matrix();
    vec4 frag_pos = model * vec4(POSITION.xyz, 1.0);
    gl_Position = camera.vp * frag_pos;
    vs_out.frag_pos = frag_pos.xyz;
    SCREEN_POS = gl_Position;
    OUT_NORMAL = transpose(inverse(mat3(model))) * normalize(NORMAL.xyz);
    OUT_UV0 = UV0;

    return vs_out;
}

ARD_ENTRY(entry)