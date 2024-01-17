#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

#define ARD_SET_BLOOM 0
#include "ard_bindings.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

void main() {
    const uvec2 dim = textureSize(screen_tex, 0).xy;

    const vec2 srcTexelSize = 1.0 / vec2(float(dim.x), float(dim.y));
    const float x = srcTexelSize.x;
    const float y = srcTexelSize.y;

    // Take 13 samples around current texel:
    // a - b - c
    // - j - k -
    // d - e - f
    // - l - m -
    // g - h - i
    // === ('e' is the current texel) ===
    const vec3 a = texture(screen_tex, vec2(UV.x - 2*x, UV.y + 2*y)).rgb;
    const vec3 b = texture(screen_tex, vec2(UV.x,       UV.y + 2*y)).rgb;
    const vec3 c = texture(screen_tex, vec2(UV.x + 2*x, UV.y + 2*y)).rgb;

    const vec3 d = texture(screen_tex, vec2(UV.x - 2*x, UV.y)).rgb;
    const vec3 e = texture(screen_tex, vec2(UV.x,       UV.y)).rgb;
    const vec3 f = texture(screen_tex, vec2(UV.x + 2*x, UV.y)).rgb;

    const vec3 g = texture(screen_tex, vec2(UV.x - 2*x, UV.y - 2*y)).rgb;
    const vec3 h = texture(screen_tex, vec2(UV.x,       UV.y - 2*y)).rgb;
    const vec3 i = texture(screen_tex, vec2(UV.x + 2*x, UV.y - 2*y)).rgb;

    const vec3 j = texture(screen_tex, vec2(UV.x - x, UV.y + y)).rgb;
    const vec3 k = texture(screen_tex, vec2(UV.x + x, UV.y + y)).rgb;
    const vec3 l = texture(screen_tex, vec2(UV.x - x, UV.y - y)).rgb;
    const vec3 m = texture(screen_tex, vec2(UV.x + x, UV.y - y)).rgb;

    // Apply weighted distribution:
    // 0.5 + 0.125 + 0.125 + 0.125 + 0.125 = 1
    // a,b,d,e * 0.125
    // b,c,e,f * 0.125
    // d,e,g,h * 0.125
    // e,f,h,i * 0.125
    // j,k,l,m * 0.5
    // This shows 5 square areas that are being sampled. But some of them overlap,
    // so to have an energy preserving downsample we need to make some adjustments.
    // The weights are the distributed, so that the sum of j,k,l,m (e.g.)
    // contribute 0.5 to the final color output. The code below is written
    // to effectively yield this sum. We get:
    // 0.125*5 + 0.03125*4 + 0.0625*4 = 1
    vec3 downsample = e*0.125;
    downsample += (a+c+g+i)*0.03125;
    downsample += (b+d+f+h)*0.0625;
    downsample += (j+k+l+m)*0.125;

    FRAGMENT_COLOR = vec4(downsample, 1);
}