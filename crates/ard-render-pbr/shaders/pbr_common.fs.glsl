/// Common interface used by all PBR fragment shaders

#ifndef _ARD_PBR_COMMON_VS
#define _ARD_PBR_COMMON_VS

/////////////////
/// VS INPUTS ///
/////////////////

// layout(location = 0) in vec3 vs_Color;
layout(location = 1) flat in uvec4 vs_Slots;

#if ARD_VS_HAS_UV0
layout(location = 2) in vec2 vs_Uv;
#endif

#ifdef COLOR_PASS
    layout(location = 3) in vec3 vs_Normal;
    layout(location = 4) in vec4 vs_Position;
    layout(location = 5) in vec3 vs_WorldSpaceFragPos;
    layout(location = 6) in vec4 vs_ViewSpacePosition;
    #if ARD_VS_HAS_TANGENT
        layout(location = 7) in mat3 vs_TBN;
    #endif
#endif

#endif