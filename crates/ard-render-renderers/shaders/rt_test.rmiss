#version 460
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_control_flow_attributes : enable
#extension GL_EXT_ray_tracing : enable

#define ARD_SET_CAMERA 0
#define ARD_SET_REFLECTION_RT_PASS 1
#include "ard_bindings.glsl"

layout(location = 0) rayPayloadInEXT ReflectionRayPayload hit_value;

void main() {
	// Sample the environment map
    const vec3 ray_dir = normalize(gl_WorldRayDirectionEXT);
    const vec3 color = texture(env_map, ray_dir).rgb;

    hit_value.hit = 0;
    hit_value.color = uvec2(
        packHalf2x16(vec2(color.rg)),
        packHalf2x16(vec2(color.b, 1.0))
    );
}