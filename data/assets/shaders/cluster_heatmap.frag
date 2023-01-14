#version 450 core
#extension GL_EXT_debug_printf : enable

#define ARD_FRAGMENT_SHADER
#include "ard_std.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 VPOS;

const vec3 EMPTY_CLUSTER = vec3(0.38, 0.99, 0.48);
const vec3 FULL_CLUSTER = vec3(0.92, 0.25, 0.2);

void entry() {
    vec2 uv = (VPOS.xy / VPOS.w) * vec2(0.5) + vec2(0.5);
    float depth = (VPOS.w * camera.near_clip) / VPOS.z;
    ivec3 cluster = ivec3(
        clamp(int(uv.x * float(FROXEL_TABLE_X)), 0, FROXEL_TABLE_X - 1),
        clamp(int(uv.y * float(FROXEL_TABLE_Y)), 0, FROXEL_TABLE_Y - 1),
        clamp(
            int(log(depth) * camera.cluster_scale_bias.x - camera.cluster_scale_bias.y), 
            0,
            FROXEL_TABLE_Z - 1
        )
    );

    uint light_count = ARD_CLUSTERS.light_counts[cluster.z][cluster.x][cluster.y];
    float scale = float(light_count) / float(MAX_LIGHTS_PER_FROXEL);

    FRAGMENT_COLOR = vec4(vec3(mix(EMPTY_CLUSTER, FULL_CLUSTER, scale)), 1.0);
}
ARD_ENTRY(entry)
