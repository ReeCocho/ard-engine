#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_REFLECTION_APPLY 0
#include "ard_bindings.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

void main() {
    const vec3 kS = texture(thin_g_tex, UV).rgb;
    vec3 refl_color = kS * texture(src_image, UV).rgb;
    FRAGMENT_COLOR = vec4(refl_color, 1.0);
}