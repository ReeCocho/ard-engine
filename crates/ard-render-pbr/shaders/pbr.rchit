#version 460
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_control_flow_attributes : enable
#extension GL_EXT_ray_tracing : enable

#define ARD_SET_CAMERA 0
#define ARD_SET_REFLECTION_RT_PASS 1
#include "ard_bindings.glsl"

layout(location = 0) rayPayloadInEXT ReflectionRayPayload hit_value;
hitAttributeEXT vec2 attribs;

void main() {
    // Compute barycentrics
    const vec3 barycentric_coords = vec3(1.0 - attribs.x - attribs.y, attribs.x, attribs.y);

    // Simple lighting.
    hit_value.hit = 1;
    hit_value.color = uvec2(
        packHalf2x16(vec2(1.0, 1.0)),
        packHalf2x16(vec2(1.0, 1.0))
    );
    hit_value.location = vec4(gl_WorldRayOriginEXT + gl_WorldRayDirectionEXT * gl_HitTEXT, 1.0);
}