#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

#define ARD_SET_GUI 0
#include "ard_bindings.glsl"

layout(location = 0) out vec4 OUT_COLOR;

layout(location = 0) in vec4 IN_COLOR;
layout(location = 1) in vec2 IN_UV;

layout(push_constant) uniform constants {
    GuiPushConstants consts;
};

void main() {
    vec4 color = vec4(0); 
    if (consts.texture_id == GUI_SCENE_TEXTURE_ID)
    {
        color = texture(scene_texture, IN_UV);
    }
    else
    {
        color = texture(font_texture, IN_UV);
    }
    
    OUT_COLOR = IN_COLOR * color;
}