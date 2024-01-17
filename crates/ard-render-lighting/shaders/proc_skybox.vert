#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_CAMERA 0
#define ARD_SET_GLOBAL 1
#include "ard_bindings.glsl"

layout(location = 0) out vec3 LOCAL_POS;

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
    LOCAL_POS = POINTS[gl_VertexIndex];
    gl_Position = camera.vp * vec4(LOCAL_POS + camera.position.xyz, 1.0);
}