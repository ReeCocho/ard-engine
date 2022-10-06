#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "data_structures.glsl"

layout(location = 0) out vec4 OUT_COLOR;

layout(set = 0, binding = 0) uniform sampler2D FONT_TEX;

layout(location = 0) in vec4 IN_COLOR;
layout(location = 1) in vec2 IN_UV;

void main() {
    OUT_COLOR = IN_COLOR * texture(FONT_TEX, IN_UV);
}