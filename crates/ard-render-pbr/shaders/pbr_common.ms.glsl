#ifndef _ARD_PBR_COMMON_MS
#define _ARD_PBR_COMMON_MS

layout(constant_id = 0) const uint MS_INVOCATIONS = MAX_PRIMITIVES;
layout(local_size_x_id = 0, local_size_y_id = 1, local_size_z_id = 2) in;

#define ITERS_PER_PRIM ((MAX_PRIMITIVES + (MS_INVOCATIONS - 1)) / MS_INVOCATIONS)
#define ITERS_PER_VERT ((MAX_VERTICES + (MS_INVOCATIONS - 1)) / MS_INVOCATIONS)

taskPayloadSharedEXT MsPayload payload;

layout(triangles, max_vertices = MAX_VERTICES, max_primitives = MAX_PRIMITIVES) out;



// layout(location = 0) out vec3 vs_Color[];
layout(location = 1) flat out uvec4 vs_Slots[];

#if ARD_VS_HAS_UV0
layout(location = 2) out vec2 vs_Uv[];
#endif

#ifdef COLOR_PASS
layout(location = 3) out vec3 vs_Normal[];

// Proj * View * Model * Position;
layout(location = 4) out vec4 vs_Position[];

// Model * Position;
layout(location = 5) out vec3 vs_WorldSpaceFragPos[];

// View * Model * Position;
layout(location = 6) out vec4 vs_ViewSpacePosition[];

#if ARD_VS_HAS_TANGENT
layout(location = 7) out mat3 vs_TBN[];
#endif
#endif

#endif