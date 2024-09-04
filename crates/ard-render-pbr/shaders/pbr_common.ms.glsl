#ifndef _ARD_PBR_COMMON_MS
#define _ARD_PBR_COMMON_MS

layout(constant_id = 0) const uint MS_INVOCATIONS = MAX_PRIMITIVES;
layout(local_size_x_id = 0, local_size_y_id = 1, local_size_z_id = 2) in;

#define ITERS_PER_PRIM ((MAX_PRIMITIVES + (MS_INVOCATIONS - 1)) / MS_INVOCATIONS)
#define ITERS_PER_VERT ((MAX_VERTICES + (MS_INVOCATIONS - 1)) / MS_INVOCATIONS)

taskPayloadSharedEXT MsPayload payload;

layout(triangles, max_vertices = MAX_VERTICES, max_primitives = MAX_PRIMITIVES) out;

layout(location = 0) out DataBlock {
    flat uvec4 slots;
#ifdef COLOR_PASS
    vec4 ndc_position;
    vec4 ndc_last_position;
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
#if defined(ENTITY_PASS)
    flat uint entity;
#endif
} vs_out[];

#endif