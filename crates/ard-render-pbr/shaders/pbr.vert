#version 450 core
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable

#define VERTEX_SHADER

#define ArdMaterialData PbrMaterial

#define ARD_SET_GLOBAL 0
#define ARD_SET_CAMERA 1
#define ARD_SET_TEXTURES 2
#define ARD_SET_MATERIALS 3

#include "ard_bindings.glsl"
#include "pbr_common.glsl"

////////////////////
/// MAIN PROGRAM ///
////////////////////

void main() {
    const uint id = ard_ObjectId;
    const mat4 model = object_data[id].model;

    const vec4 ws_frag_pos = model * ard_Position;
    const vec4 position = camera.vp * ws_frag_pos;
    vs_ViewSpacePosition = camera.view * ws_frag_pos;

    gl_Position = position;
    vs_Position = position;
    vs_WorldSpaceFragPos = ws_frag_pos.xyz;

    // Compute TBN if we have tangents and UVs (UVs are required as well because the TBN is only
    // used when doing normal mapping.
#if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0
    vec3 T = normalize(vec3(model * ard_Tangent));
    vec3 N = normalize(vec3(model * ard_Normal));
    T = normalize(T - dot(T, N) * N);
    vec3 B = cross(N, T);
    
    vs_TBN = mat3(T, B, N);
#else
    // Output corrected normal
    const mat3 normal_mat = mat3(object_data[id].normal);
    vs_Normal = normalize(normal_mat * ard_Normal.xyz);
#endif
    
#if ARD_VS_HAS_UV0
    vs_Slots.x = object_data[id].textures;
    vs_Uv = ard_Uv0;
#endif
    vs_Slots.y = object_data[id].material;
}