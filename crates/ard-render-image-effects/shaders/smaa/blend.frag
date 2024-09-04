#version 450 core
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_control_flow_attributes : enable

#define ARD_SET_SMAA_BLEND 0
#include "utils.glsl"
#include "ard_bindings.glsl"

#define SMAA_RT_METRICS (consts.rt_metrics)
#define mad(a, b, c) (a * b + c)

layout(push_constant) uniform constants {
    SmaaPushConstants consts;
};

layout(location = 0) out vec4 OUT_COLOR;

layout(location = 0) in vec2 tex_coord;
layout(location = 1) in vec4 offset;


void SMAAMovc(bvec2 cond, inout vec2 variable, vec2 value) {
    if (cond.x) variable.x = value.x;
    if (cond.y) variable.y = value.y;
}

void SMAAMovc(bvec4 cond, inout vec4 variable, vec4 value) {
    SMAAMovc(cond.xy, variable.xy, value.xy);
    SMAAMovc(cond.zw, variable.zw, value.zw);
}

void main() {
    if (consts.edge_viz != 0) {
        OUT_COLOR = vec4(texture(blend_tex, tex_coord).rgb, 1.0);
        return;
    }

    vec4 color;

    // Fetch the blending weights for current pixel:
    vec4 a;
    a.x = texture(blend_tex, offset.xy).a; // Right
    a.y = texture(blend_tex, offset.zw).g; // Top
    a.wz = texture(blend_tex, tex_coord).xz; // Bottom / Left

    // Is there any blending weight with a value greater than 0.0?
    if (dot(a, vec4(1.0, 1.0, 1.0, 1.0)) <= 1e-5) {
        color = texture(src_tex, tex_coord); // LinearSampler
    } else {
        bool h = max(a.x, a.z) > max(a.y, a.w); // max(horizontal) > max(vertical)

        // Calculate the blending offsets:
        vec4 blendingOffset = vec4(0.0, a.y, 0.0, a.w);
        vec2 blendingWeight = a.yw;
        SMAAMovc(bvec4(h, h, h, h), blendingOffset, vec4(a.x, 0.0, a.z, 0.0));
        SMAAMovc(bvec2(h, h), blendingWeight, a.xz);
        blendingWeight /= dot(blendingWeight, vec2(1.0, 1.0));

        // Calculate the texture coordinates:
        vec4 blendingCoord = mad(blendingOffset, vec4(SMAA_RT_METRICS.xy, -SMAA_RT_METRICS.xy), tex_coord.xyxy);

        // We exploit bilinear filtering to mix current pixel with the chosen
        // neighbor:
        color = blendingWeight.x * texture(src_tex, blendingCoord.xy); // LinearSampler
        color += blendingWeight.y * texture(src_tex, blendingCoord.zw); // LinearSampler
    }

    OUT_COLOR = vec4(color.rgb, 1.0);
}