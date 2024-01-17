/// Common interface used by all PBR vertex shaders

#ifndef _ARD_PBR_COMMON_VS
#define _ARD_PBR_COMMON_VS

layout(location = 0) in vec4 ard_Position;
layout(location = 1) in vec4 ard_Normal;

#if ARD_VS_HAS_TANGENT
    layout(location = 1 + ARD_VS_HAS_TANGENT) 
    in vec4 ard_Tangent;
#endif

#if ARD_VS_HAS_COLOR
    layout(
        location = 1 
        + ARD_VS_HAS_TANGENT 
        + ARD_VS_HAS_COLOR
    ) in vec4 ard_Color;
#endif

#if ARD_VS_HAS_UV0
    layout(
        location = 1 
        + ARD_VS_HAS_TANGENT 
        + ARD_VS_HAS_COLOR 
        + ARD_VS_HAS_UV0
    ) in vec2 ard_Uv0;
#endif

#if ARD_VS_HAS_UV1
    layout(
        location = 1 
        + ARD_VS_HAS_TANGENT 
        + ARD_VS_HAS_COLOR 
        + ARD_VS_HAS_UV0 
        + ARD_VS_HAS_UV1
    ) in vec2 ard_Uv1;
#endif

#if ARD_VS_HAS_UV2
    layout(
        location = 1 
        + ARD_VS_HAS_TANGENT 
        + ARD_VS_HAS_COLOR 
        + ARD_VS_HAS_UV0 
        + ARD_VS_HAS_UV1 
        + ARD_VS_HAS_UV2
    ) in vec2 ard_Uv2;
#endif

#if ARD_VS_HAS_UV3
    layout(
        location = 1 
        + ARD_VS_HAS_TANGENT 
        + ARD_VS_HAS_COLOR 
        + ARD_VS_HAS_UV0 
        + ARD_VS_HAS_UV1 
        + ARD_VS_HAS_UV2 
        + ARD_VS_HAS_UV3
    ) in vec2 ard_Uv3;
#endif

//////////////////
/// VS OUTPUTS ///
//////////////////

layout(location = 0) out vec3 vs_Normal;
layout(location = 1) flat out uint vs_TextureSlotsIdx;
layout(location = 2) flat out uint vs_MaterialDataSlotIdx;

#if ARD_VS_HAS_UV0
layout(location = 3) out vec2 vs_Uv;
#endif

#if ARD_VS_HAS_TANGENT
layout(location = 4) out mat3 vs_TBN;
#endif

layout(location = 8) out vec4 vs_Position;
layout(location = 9) out vec3 vs_WorldSpaceFragPos;
layout(location = 10) out vec4 vs_ViewSpacePosition;
layout(location = 11) out vec4 vs_LightSpacePositions[MAX_SHADOW_CASCADES];

#endif