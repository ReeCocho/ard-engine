#version 460
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_control_flow_attributes : enable
#extension GL_EXT_ray_tracing : enable

#define ARD_SET_PATH_TRACER_PASS 0
#define ARD_SET_CAMERA 1
#include "ard_bindings.glsl"

layout(location = 0) rayPayloadInEXT PathTracerPayload hit_value;

void main() {
	// Sample the environment map
    const vec3 ray_dir = normalize(gl_WorldRayDirectionEXT);
    vec3 color = texture(env_map, ray_dir).rgb;

    hit_value.hit = 0;
    hit_value.color = vec4(color, 1.0);
    hit_value.location = vec4(0.0);
}