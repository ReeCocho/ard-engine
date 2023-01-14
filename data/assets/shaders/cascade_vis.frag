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

    if (ARD_SHADOW_INFO.cascade_count == 0) {
        FRAGMENT_COLOR = vec4(SLICE_COLORS[0], 1.0);
        return;
    }
    
    // Determine which cascade to use
    int layer = int(ARD_SHADOW_INFO.cascade_count) - 1;
    for (int i = 0; i < ARD_SHADOW_INFO.cascade_count; ++i) {
        if (depth < ARD_SHADOW_INFO.cascades[i].far_plane) {
            layer = i;
            break;
        }
    }

    FRAGMENT_COLOR = vec4(SLICE_COLORS[layer % 7], 1.0);
}
ARD_ENTRY(entry)
