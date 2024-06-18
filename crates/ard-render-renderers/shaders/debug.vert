#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_CAMERA 0
#include "ard_bindings.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 COLOR;

layout(location = 0) out vec4 OUT_COLOR;

void main() {
    gl_Position = camera[0].vp * POSITION;
    OUT_COLOR = COLOR;
}