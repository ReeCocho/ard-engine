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

// X = Color, Y = Metallic/Roughness, Z = Normal, W = Material
layout(location = 0) flat out uvec4 vs_Slots;

#if ARD_VS_HAS_UV0
layout(location = 1) out vec2 vs_Uv;
#endif

#ifdef WITH_NORMALS
    layout(location = 2) out vec3 vs_Normal;
#endif

#ifndef DEPTH_ONLY
    layout(location = 2) out vec3 vs_Normal;

    // Proj * View * Model * Position;
    layout(location = 3) out vec4 vs_Position;

    // Model * Position;
    layout(location = 4) out vec3 vs_WorldSpaceFragPos;

    // View * Model * Position;
    layout(location = 5) out vec4 vs_ViewSpacePosition;

    #if ARD_VS_HAS_TANGENT
    layout(location = 6) out mat3 vs_TBN;
    #endif
#endif

#endif