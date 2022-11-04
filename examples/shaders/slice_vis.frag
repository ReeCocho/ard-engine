#version 450 core
#extension GL_EXT_debug_printf : enable

#define ARD_FRAGMENT_SHADER
#include "ard_std.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 VPOS;

const vec3 SLICE_COLORS[] = vec3[7](
    vec3(1.0, 0.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 0.0, 1.0),
    vec3(1.0, 1.0, 0.0),
    vec3(1.0, 0.0, 1.0),
    vec3(0.0, 1.0, 1.0),
    vec3(1.0, 1.0, 1.0)
);

void entry() {
    float depth = (VPOS.w * camera.near_clip) / VPOS.z;
    int slice = clamp(
        int(log(depth) * camera.cluster_scale_bias.x - camera.cluster_scale_bias.y), 
        0,
        FROXEL_TABLE_Z - 1
    );
    FRAGMENT_COLOR = vec4(SLICE_COLORS[slice % 7], 1.0);
}
ARD_ENTRY(entry)
