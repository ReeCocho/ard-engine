#version 450 core
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_scalar_block_layout : enable

#define ARD_SET_CAMERA 0
#include "ard_bindings.glsl"
#include "skybox.glsl"
#include "utils.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

#if defined(COLOR_PASS)
layout(location = 1) out vec4 OUT_KS_RGH;
layout(location = 2) out vec4 OUT_VEL;
layout(location = 3) out vec4 OUT_NORM;
layout(location = 4) out vec4 OUT_TAN;
#endif

layout(location = 0) in vec3 DIR;
#if defined(COLOR_PASS)
layout(location = 1) in vec4 CUR_POS;
layout(location = 2) in vec4 PRV_POS;
#endif

layout(push_constant) uniform constants {
    SkyBoxRenderPushConstants consts;
};

void main() {
    const vec3 d = normalize(DIR);
    FRAGMENT_COLOR = vec4(
        skybox_color(d, normalize(consts.sun_direction.xyz)), 
        1.0
    );
#if defined(COLOR_PASS)
    vec2 cur_pos = CUR_POS.xy / CUR_POS.w;
    cur_pos.y = -cur_pos.y;
    cur_pos = (cur_pos + vec2(1.0)) * vec2(0.5);

    vec2 prv_pos = PRV_POS.xy / PRV_POS.w;
    prv_pos.y = -prv_pos.y;
    prv_pos = (prv_pos + vec2(1.0)) * vec2(0.5);

    OUT_KS_RGH = vec4(0.0);
    OUT_VEL = vec4(cur_pos - prv_pos, 0.0, 0.0);
    OUT_NORM = vec4(0.0);
    OUT_TAN = vec4(0.0);
#endif
}