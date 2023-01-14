#version 450 core

#define ARD_VERTEX_SHADER
#include "ard_std.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 NORMAL;
layout(location = 2) in vec4 TANGENT;
layout(location = 3) in vec2 UV0;

layout(location = 0) out vec4 VPOS;
layout(location = 1) out vec4 OUT_NORMAL;
layout(location = 2) out vec2 UV;
layout(location = 3) out mat3 TBN;

VsOut entry() {
    VsOut vs_out;
    mat4 model = MODEL_MATRIX;
    vec4 frag_pos = model * vec4(POSITION.xyz, 1.0);
    gl_Position = camera.vp * frag_pos;
    VPOS = gl_Position;

    vec3 T = normalize(vec3(model * vec4(normalize(TANGENT.xyz), 0.0)));
    vec3 N = normalize(vec3(model * vec4(normalize(NORMAL.xyz), 0.0)));
    T = normalize(T - dot(T, N) * N);
    vec3 B = cross(N, T);
    TBN = mat3(T, B, N);

    OUT_NORMAL = vec4(normalize((NORMAL_MATRIX * vec4(normalize(NORMAL.xyz), 0.0)).xyz), 0.0);
    UV = UV0;
    vs_out.frag_pos = frag_pos.xyz;

    return vs_out;
}
// ARD_ENTRY(entry)

void main() {
    uint idx = ARD_OBJECT_INDICES[gl_InstanceIndex];
    ARD_INSTANCE_IDX = gl_InstanceIndex;
    ARD_MATERIAL_IDX = ARD_OBJECT_DATA[idx].material;
    ARD_TEXTURES_IDX = ARD_OBJECT_DATA[idx].textures;
    VsOut vs_out = entry();
    ARD_FRAG_POS = vs_out.frag_pos;
    ARD_FRAG_POS_VIEW_SPACE = vec3(camera.view * vec4(vs_out.frag_pos, 1.0));
    for (int i = 0; i < ARD_SHADOW_INFO.cascade_count; ++i) {
        ARD_FRAG_POS_LIGHT_VIEW_SPACE[i] =
            vec3(ARD_SHADOW_INFO.cascades[i].view * vec4(vs_out.frag_pos, 1.0));
        ARD_FRAG_POS_LIGHT_SPACE[i] = ARD_SHADOW_INFO.cascades[i].vp * vec4(vs_out.frag_pos, 1.0);
    }
}