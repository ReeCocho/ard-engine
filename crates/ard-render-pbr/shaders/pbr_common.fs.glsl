/// Common interface used by all PBR fragment shaders

#ifndef _ARD_PBR_COMMON_VS
#define _ARD_PBR_COMMON_VS

/////////////////
/// VS INPUTS ///
/////////////////

layout(location = 0) in DataBlock {
    flat uvec4 slots;
#ifdef COLOR_PASS
    vec4 ndc_position;
    vec4 view_space_position;
#endif
    vec3 world_space_position;
    vec3 normal;
#if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0
    vec3 tangent;
    vec3 bitangent;
#endif
#if ARD_VS_HAS_UV0
    vec2 uv;
#endif
} vs_in;

#endif