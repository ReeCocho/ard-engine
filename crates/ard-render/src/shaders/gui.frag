#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "data_structures.glsl"

layout(location = 0) out vec4 OUT_COLOR;

layout(set = 0, binding = 0) uniform sampler2D FONT_TEX;
layout(set = 0, binding = 1) uniform sampler2D SCENE_TEX;

layout(set = 1, binding = 0) uniform sampler2D[] ARD_TEXTURES;

layout(location = 0) in vec4 IN_COLOR;
layout(location = 1) in vec2 IN_UV;

layout(push_constant) uniform constants {
    vec2 screen_size;
    uint texture_id;
};

void main() {
    vec4 color = vec4(0);
    if (texture_id == 4294967295) {
        color = texture(FONT_TEX, IN_UV);
    } 
    else if (texture_id == 4294967294) {
        color = texture(SCENE_TEX, vec2(1.0) - IN_UV);
    } else {
        color = texture(ARD_TEXTURES[texture_id], IN_UV);
    }

    OUT_COLOR = IN_COLOR * color;
}