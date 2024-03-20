#version 450 core
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_control_flow_attributes : enable

#define ARD_SET_SMAA_BLEND 0
#include "utils.glsl"
#include "ard_bindings.glsl"

layout(push_constant) uniform constants {
    SmaaPushConstants consts;
};

const vec3 POINTS[] = vec3[3](
    vec3(-1.0, -1.0, 0.0),
    vec3(-1.0, 3.0, 0.0),
    vec3(3.0, -1.0, 0.0)
);

layout(location = 0) out vec2 tex_coord;
layout(location = 1) out vec4 offset;

void main() {
    vec3 position = POINTS[gl_VertexIndex];

    vec2 uv = (position.xy * 0.5) + vec2(0.5);
    uv.y = 1.0 - uv.y;

    tex_coord = uv;
    offset = uv.xyxy + (consts.rt_metrics.xyxy * vec4(1.0, 0.0, 0.0, 1.0));
    gl_Position = vec4(position, 1.0);
}