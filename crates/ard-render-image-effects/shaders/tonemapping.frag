#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

#define ARD_SET_TONEMAPPING 0
#include "ard_bindings.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

layout(push_constant) uniform constants {
    ToneMappingPushConstants consts;
};

void main() {
    vec3 color = texture(screen_tex, UV).rgb;
    color = vec3(1.0) - exp(-color * (consts.exposure / luminance));
    color = pow(color, vec3(1.0 / consts.gamma));

    FRAGMENT_COLOR = vec4(color, 1.0);
}