#version 450 core
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_CAMERA 0
#include "ard_bindings.glsl"
#include "pbr_common.glsl"

layout(location = 0) in vec4 POSITION;

void main() {
    gl_Position = camera.vp * POSITION;
}