#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_CAMERA 0
#define ARD_SET_GLOBAL 1
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
    const vec4 pos = camera.vp * vec4(DIR + camera.position.xyz, 1.0);
    gl_Position = vec4(pos.xy, 0.0, pos.w);
}