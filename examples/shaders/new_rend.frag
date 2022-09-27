#version 450 core
#extension GL_EXT_debug_printf : enable

#define ARD_FRAGMENT_SHADER
#include "ard_std.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 VERT_COLOR;
layout(location = 1) in vec4 SCREEN_POS;

const vec3 SLICE_TO_COLOR[FROXEL_TABLE_Z] = vec3[FROXEL_TABLE_Z](
    vec3(0.1),
    vec3(1.0),
    vec3(0.5),
    vec3(1.0, 0.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 0.0, 1.0),
    vec3(1.0, 1.0, 0.0),
    vec3(1.0, 0.0, 1.0),
    vec3(0.0, 1.0, 1.0),
    vec3(0.5, 0.0, 0.0),
    vec3(0.0, 0.5, 0.0),
    vec3(0.0, 0.0, 0.5),
    vec3(0.5, 0.5, 0.0),
    vec3(0.5, 0.0, 0.5),
    vec3(0.0, 0.5, 0.5),
    vec3(0.9)
);

void entry() {
    vec3 world_pos = ARD_FRAG_POS;
    vec2 uv = ((SCREEN_POS.xy / SCREEN_POS.w) * 0.5) + vec2(0.5);
    ivec3 cluster = ivec3(
        clamp(int(uv.x * float(FROXEL_TABLE_X)), 0, FROXEL_TABLE_X - 1),
        clamp(int(uv.y * float(FROXEL_TABLE_Y)), 0, FROXEL_TABLE_Y - 1),
        clamp(
            int(log(SCREEN_POS.z) * camera.cluster_scale_bias.x - camera.cluster_scale_bias.y), 
            0, 
            FROXEL_TABLE_Z - 1
        )
    );

    vec3 color = vec3(0.1);
    uint count = ARD_CLUSTERS.light_counts[cluster.z][cluster.x][cluster.y];
    for (int i = 0; i < count; i++) {
        uint light_idx = ARD_CLUSTERS.clusters[cluster.z][cluster.x][cluster.y][i];
        debugPrintfEXT("%u %u", count, light_idx);
        Light light = ARD_LIGHTS[light_idx];
        float dist = length(light.position_range.xyz - world_pos);
        float sqr_dist = dist * dist;
        float sqr_range = light.position_range.w * light.position_range.w;
        float attenuation = (1.0 - (sqr_dist / sqr_range)) * light.color_intensity.w;
        color += vec3(clamp(attenuation, 0.0, 1.0));
    }
    // color = vec3(count / 8.0);

    FRAGMENT_COLOR = vec4(color, 1.0);
}
ARD_ENTRY(entry)
