/// Common interface used by all PBR fragment shaders

#ifndef _ARD_PBR_COMMON_VS
#define _ARD_PBR_COMMON_VS

/////////////////
/// VS INPUTS ///
/////////////////

layout(location = 0) in vec3 vs_Normal;
layout(location = 1) flat in uint vs_TextureSlotsIdx;
layout(location = 2) flat in uint vs_MaterialSlotIdx;

#if ARD_VS_HAS_UV0
layout(location = 3) in vec2 vs_Uv;
#endif

#if ARD_VS_HAS_TANGENT
layout(location = 4) in mat3 vs_TBN;
#endif

layout(location = 8) in vec4 vs_Position;
layout(location = 9) in vec3 vs_WorldSpaceFragPos;
layout(location = 10) in vec4 vs_ViewSpacePosition;
layout(location = 11) in vec4 vs_LightSpacePositions[MAX_SHADOW_CASCADES];

#endif