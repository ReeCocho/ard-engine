#version 450 core
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable

#define VERTEX_SHADER

#define ArdMaterialData PbrMaterial

#define ARD_SET_GLOBAL 0
#define ARD_SET_TEXTURES 1
#define ARD_SET_CAMERA 2
#define ARD_SET_MATERIALS 3

#include "ard_bindings.glsl"
#include "pbr_common.glsl"

////////////////////
/// MAIN PROGRAM ///
////////////////////

void main() {
    const uint id = ard_ObjectId;
    const mat4 model_mat = ard_ModelMatrix(id);
    const mat3 normal_mat = mat3(ard_NormalMatrix(id));

    vec4 position = camera.vp * model_mat * ard_Position;
    gl_Position = position;
    vs_Position = position;

    // Compute TBN if we have tangents
#if ARD_VS_HAS_TANGENT
    vec3 T = normalize(vec3(model_mat * ard_Tangent));
    vec3 N = normalize(vec3(model_mat * ard_Normal));
    T = normalize(T - dot(T, N) * N);
    vec3 B = cross(N, T);
    vs_TBN = mat3(T, B, N);
#endif

    // Output corrected normal
    vs_Normal = normalize(normal_mat * ard_Normal.xyz);
    
    // Bindless resource IDs
    vs_TextureSlotsIdx = ard_TextureSlot(id);
    vs_MaterialDataSlotIdx = ard_MaterialSlot(id);
    
    // Output UVs if we have them
#if ARD_VS_HAS_UV0
    vs_Uv = ard_Uv0;
#endif

}