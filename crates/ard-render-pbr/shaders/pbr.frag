#version 450 core
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable

#define FRAGMENT_SHADER

#define ArdMaterialData PbrMaterial

#define ARD_SET_GLOBAL 0
#define ARD_SET_TEXTURES 1
#define ARD_SET_CAMERA 2
#define ARD_SET_MATERIALS 3

#define ARD_TEXTURE_COUNT 1

#include "ard_bindings.glsl"
#include "pbr_common.glsl"

#ifndef DEPTH_ONLY
layout(location = 0) out vec4 OUT_COLOR;
#endif

////////////////////
/// MAIN PROGRAM ///
////////////////////

void main() {
    PbrMaterial data = get_material_data();

#if ARD_VS_HAS_UV0
    vec4 color = sample_texture_default(0, vs_Uv, vec4(1));
#else
    vec4 color = vec4(1, 1, 1, 1);
#endif

    // Alpha-Cutoff
    if (color.a < data.alpha_cutoff) {
        discard;
    }

    // vec3 normal = normalize(vs_Normal);
#ifndef DEPTH_ONLY
    const ivec3 cluster = get_cluster_id(vs_Position);
    const uint light_count = light_table.counts[cluster.z][cluster.x][cluster.y];

    OUT_COLOR = light_count > 0 ? vec4(1) : data.color * color;
#endif
}