#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

#define ARD_SET_BLOOM 0
#include "ard_bindings.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

void main(){
    // The filter kernel is applied with a radius, specified in texture
    // coordinates, so that the radius will vary across mip resolutions.
    const float x = 0.003; // filterRadius;
    const float y = 0.003; // filterRadius;

    // Take 9 samples around current texel:
    // a - b - c
    // d - e - f
    // g - h - i
    // === ('e' is the current texel) ===
    const vec3 a = texture(screen_tex, vec2(UV.x - x, UV.y + y)).rgb;
    const vec3 b = texture(screen_tex, vec2(UV.x,     UV.y + y)).rgb;
    const vec3 c = texture(screen_tex, vec2(UV.x + x, UV.y + y)).rgb;

    const vec3 d = texture(screen_tex, vec2(UV.x - x, UV.y)).rgb;
    const vec3 e = texture(screen_tex, vec2(UV.x,     UV.y)).rgb;
    const vec3 f = texture(screen_tex, vec2(UV.x + x, UV.y)).rgb;

    const vec3 g = texture(screen_tex, vec2(UV.x - x, UV.y - y)).rgb;
    const vec3 h = texture(screen_tex, vec2(UV.x,     UV.y - y)).rgb;
    const vec3 i = texture(screen_tex, vec2(UV.x + x, UV.y - y)).rgb;

    // Apply weighted distribution, by using a 3x3 tent filter:
    //  1   | 1 2 1 |
    // -- * | 2 4 2 |
    // 16   | 1 2 1 |
    vec3 upsample = e*4.0;
    upsample += (b+d+f+h)*2.0;
    upsample += (a+c+g+i);
    upsample *= 1.0 / 16.0;

    FRAGMENT_COLOR = vec4(upsample, 1.0);
}