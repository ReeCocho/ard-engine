#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_TONEMAPPING 0
#define ARD_SET_CAMERA 1
#include "ard_bindings.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

layout(push_constant) uniform constants {
    ToneMappingPushConstants consts;
};

void main() {
    vec3 color = texture(screen_tex, UV).rgb;
    vec3 bloom = texture(bloom_image, UV).rgb;
    vec3 sun_shafts = texture(sun_shafts_image, UV).rgb;

    // Tonemapping with adaptive luminance
    // color = mix(color, bloom, 0.05);
    color += 0.2 * sun_shafts;
    color = vec3(1.0) - exp(-color * (consts.exposure / luminance));
    color = pow(color, vec3(1.0 / consts.gamma));

    FRAGMENT_COLOR = vec4(color, 1.0);
}