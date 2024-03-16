#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_CAMERA 0
#define ARD_SET_ENV_PREFILTER 1
#include "ard_bindings.glsl"

const float PI = 3.14159265359;

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec3 DIR;

layout(push_constant) uniform constants {
    EnvPrefilterPushConstants consts;
};

void main() {
    const vec3 N = normalize(DIR.xyz);
    const vec3 R = N;
    const vec3 V = R;

    vec3 prefiltered_color = vec3(0.0);

    // from tangent-space vector to world-space sample vector
    vec3 up        = abs(N.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
    vec3 tangent   = normalize(cross(up, N));
    vec3 bitangent = cross(N, tangent);
    mat3 change_basis = mat3(tangent, bitangent, N);

    for (uint i = 0; i < ENV_PREFILTER_SAMPLE_COUNT; i++) {
        const vec3 L = normalize(change_basis * prefilter_info.halfway_vectors[i].xyz);
        prefiltered_color += 
            texture(env_map, L, prefilter_info.mip_levels[i]).rgb 
            * prefilter_info.sample_weights[i];
    }
    prefiltered_color *= prefilter_info.inv_total_sample_weight;

    FRAGMENT_COLOR = vec4(prefiltered_color, 1.0);
}