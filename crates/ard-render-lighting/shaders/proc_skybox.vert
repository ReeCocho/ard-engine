#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_multiview : enable

#define ARD_SET_CAMERA 0
#include "ard_bindings.glsl"

layout(location = 0) out vec3 DIR;

const vec3 POINTS[] = vec3[36](
    // East
    vec3(1.0, -1.0,  1.0),
    vec3(1.0,  1.0,  1.0),
    vec3(1.0,  1.0, -1.0),
    
    vec3(1.0, -1.0,  1.0),
    vec3(1.0, -1.0, -1.0),
    vec3(1.0,  1.0, -1.0),

    // West
    vec3(-1.0, -1.0,  1.0),
    vec3(-1.0,  1.0,  1.0),
    vec3(-1.0,  1.0, -1.0),
    
    vec3(-1.0, -1.0,  1.0),
    vec3(-1.0, -1.0, -1.0),
    vec3(-1.0,  1.0, -1.0),
    // North
    vec3(-1.0, -1.0, 1.0),
    vec3(-1.0,  1.0, 1.0),
    vec3( 1.0,  1.0, 1.0),

    vec3(-1.0, -1.0, 1.0),
    vec3( 1.0,  1.0, 1.0),
    vec3( 1.0, -1.0, 1.0),
    
    // South
    vec3(-1.0, -1.0, -1.0),
    vec3(-1.0,  1.0, -1.0),
    vec3( 1.0,  1.0, -1.0),

    vec3(-1.0, -1.0, -1.0),
    vec3( 1.0,  1.0, -1.0),
    vec3( 1.0, -1.0, -1.0),
    
    // Top
    vec3( 1.0, 1.0,  1.0),
    vec3(-1.0, 1.0,  1.0),
    vec3(-1.0, 1.0, -1.0),

    vec3( 1.0, 1.0,  1.0),
    vec3(-1.0, 1.0, -1.0),
    vec3( 1.0, 1.0, -1.0),

    // Bottom
    vec3( 1.0, -1.0,  1.0),
    vec3(-1.0, -1.0,  1.0),
    vec3(-1.0, -1.0, -1.0),

    vec3( 1.0, -1.0,  1.0),
    vec3(-1.0, -1.0, -1.0),
    vec3( 1.0, -1.0, -1.0)
);

void main() {
    DIR = POINTS[gl_VertexIndex];
    const vec4 pos = camera[gl_ViewIndex].vp * vec4(DIR + camera[gl_ViewIndex].position.xyz, 1.0);
    gl_Position = vec4(pos.xy, 0.0, pos.w);
}