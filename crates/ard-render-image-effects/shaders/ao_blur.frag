#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

#define ARD_SET_AO_GAUSS_BLUR 0
#include "ard_bindings.glsl"

layout(location = 0) out float FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

layout(push_constant) uniform constants {
    AoGaussBlurPushConstants consts;
};

void main() {
    const vec2 resolution = vec2(textureSize(input_tex, 0).xy);

    float ao = 0.0;
    const vec2 off1 = vec2(1.3333333333333333) * consts.direction;
    ao += texture(input_tex, UV).r * 0.29411764705882354;
    ao += texture(input_tex, UV + (off1 / resolution)).r * 0.35294117647058826;
    ao += texture(input_tex, UV - (off1 / resolution)).r * 0.35294117647058826;

    FRAGMENT_COLOR = ao;
}