#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_CAMERA 0
#include "ard_bindings.glsl"
#include "skybox.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec3 DIR;

layout(push_constant) uniform constants {
    SkyBoxRenderPushConstants consts;
};

void main() {
    FRAGMENT_COLOR = vec4(
        skybox_color(normalize(DIR), normalize(consts.sun_direction.xyz)), 
        1.0
    );
}