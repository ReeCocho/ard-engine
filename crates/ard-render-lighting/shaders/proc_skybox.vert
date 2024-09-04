#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_multiview : enable

#define ARD_SET_CAMERA 0
#include "ard_bindings.glsl"

layout(location = 0) out vec3 DIR;
#if defined(COLOR_PASS)
layout(location = 1) out vec4 CUR_POS;
layout(location = 2) out vec4 PRV_POS;
#endif

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
    const vec3 cur_pos = DIR + camera[gl_ViewIndex].position.xyz;
    const vec4 ndc_pos = camera[gl_ViewIndex].vp * vec4(cur_pos, 1.0);
#if defined(COLOR_PASS)
    const vec3 prv_pos = DIR + camera[gl_ViewIndex].last_position.xyz;
    const vec4 prv_ndc_pos = camera[gl_ViewIndex].last_vp * vec4(prv_pos, 1.0);
    CUR_POS = ndc_pos;
    PRV_POS = prv_ndc_pos;
#endif

    gl_Position = vec4(ndc_pos.xy, 0.0, ndc_pos.w);
}