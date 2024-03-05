#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "ard_types.glsl"

layout(location = 0) in vec2 POSITION;
layout(location = 1) in vec2 UV;
layout(location = 2) in vec4 COLOR;

layout(location = 0) out vec4 OUT_COLOR;
layout(location = 1) out vec2 OUT_UV;

layout(push_constant) uniform constants {
    GuiPushConstants consts;
};

void main() {
    gl_Position = vec4(
        2.0 * POSITION.x / consts.screen_size.x - 1.0, 
        1.0 - 2.0 * POSITION.y / consts.screen_size.y,
        0.0, 
        1.0
    );
    OUT_COLOR = COLOR;
    OUT_UV = UV;
}