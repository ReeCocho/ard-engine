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
    // Prefetch properties
    const uint id = ard_ObjectId;
    const mat4 model = object_data[id].model;
#if !(ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0)
    const mat3 normal_mat = mat3(object_data[id].normal);
#endif
    const uint materials_slot = object_data[id].material;
#if ARD_VS_HAS_UV0
    const uint textures_slot = object_data[id].textures;
    const uint color_slot = texture_slots[textures_slot][0];
    #if ARD_VS_HAS_TANGENT
        const uint normal_slot = texture_slots[textures_slot][1];
    #endif
    const uint met_rough_slot = texture_slots[textures_slot][2];
#endif

    const vec4 ws_frag_pos = model * ard_Position;
    const vec4 position = camera.vp * ws_frag_pos;
#ifndef DEPTH_ONLY
    vs_ViewSpacePosition = camera.view * ws_frag_pos;
#endif

    gl_Position = position;
#ifndef DEPTH_ONLY
    vs_Position = position;
    vs_WorldSpaceFragPos = ws_frag_pos.xyz;
#endif

    // Compute TBN if we have tangents and UVs (UVs are required as well because the TBN is only
    // used when doing normal mapping.
#ifndef DEPTH_ONLY
    #if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0
        vec3 T = normalize(vec3(model * ard_Tangent));
        vec3 N = normalize(vec3(model * ard_Normal));
        T = normalize(T - dot(T, N) * N);
        vec3 B = cross(N, T);
        
        vs_TBN = mat3(T, B, N);
    #else
        // Output corrected normal
        vs_Normal = normalize(normal_mat * ard_Normal.xyz);
    #endif
#endif
    
#if ARD_VS_HAS_UV0
    vs_Uv = ard_Uv0;
    #if ARD_VS_HAS_TANGENT
        vs_Slots = uvec4(color_slot, met_rough_slot, normal_slot, materials_slot);
    #else
        vs_Slots = uvec4(color_slot, met_rough_slot, EMPTY_TEXTURE_ID, materials_slot);
    #endif
#else
    vs_Slots = uvec4(EMPTY_TEXTURE_ID, EMPTY_TEXTURE_ID, EMPTY_TEXTURE_ID, materials_slot);
#endif
}