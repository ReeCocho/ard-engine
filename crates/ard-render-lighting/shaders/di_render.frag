#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_CAMERA 0
#define ARD_SET_DI_RENDER 1
#include "ard_bindings.glsl"
#include "skybox.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec3 DIR;

void main() {
    const mat4 Mr = prefiltering_mats.red;
    const mat4 Mg = prefiltering_mats.green;
    const mat4 Mb = prefiltering_mats.blue;

    // Matrices expect Z up vectors
    vec4 norm = vec4(normalize(DIR.xyz), 1.0);
    norm = vec4(norm.x, norm.z, norm.y, norm.w);

    const vec3 color = vec3(
        dot(norm, Mr * norm),
        dot(norm, Mg * norm),
        dot(norm, Mb * norm)
    );

    FRAGMENT_COLOR = vec4(max(color, 0.0), 1.0);
}