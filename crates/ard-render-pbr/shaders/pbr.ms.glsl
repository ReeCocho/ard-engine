#version 450 core
#pragma shader_stage(mesh)
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_multiview : enable
#extension GL_EXT_mesh_shader: require
#extension GL_EXT_control_flow_attributes : enable

#define ARD_TEXTURE_COUNT 3
#define MESH_SHADER
#define ArdMaterialData PbrMaterial
#include "pbr_common.glsl"
#include "utils.glsl"

void main() {
    // Extract shared values
    const uint object_id = payload.object_ids[gl_WorkGroupID.x];
    const uint index_offset = payload.index_offsets[gl_WorkGroupID.x];
    const uint vertex_offset = payload.vertex_offsets[gl_WorkGroupID.x];
    const uint vp_count = payload.counts[gl_WorkGroupID.x];
    const uint vert_count = vp_count & 0xFF;
    const uint prim_count = (vp_count >> 8) & 0xFF;

    // Allocate outputs
    if (gl_LocalInvocationIndex == 0) {
        SetMeshOutputsEXT(vert_count, prim_count);
    }

    // Fetch properties
    const mat4 model = object_data[object_id].model;
    const mat3 normal_mat = mat3(object_data[object_id].normal);
    const uint materials_slot = object_data[object_id].material;

#if ARD_VS_HAS_UV0
    const uint textures_slot = object_data[object_id].textures;
    const uint color_slot = texture_slots[textures_slot][0];
#if ARD_VS_HAS_TANGENT
    const uint normal_slot = texture_slots[textures_slot][1];
#endif
    const uint met_rough_slot = texture_slots[textures_slot][2];
#endif

    // Generate primitives
    [[unroll]]
    for (uint i = 0; i < ITERS_PER_PRIM; i++) {
        // Fetch primitive index and break early if OOB
        const uint prim_idx = (MS_INVOCATIONS * i) + gl_LocalInvocationIndex;
        if (prim_idx >= prim_count) {
            continue;
        }

        // Read in indices
        const uint base = index_offset + (prim_idx * 3);
        const uint i1 = v_indices[base];
        const uint i2 = v_indices[base + 1];
        const uint i3 = v_indices[base + 2];

        const uvec3 prim = uvec3(
            i1 - vertex_offset,
            i2 - vertex_offset,
            i3 - vertex_offset
        );

        gl_PrimitiveTriangleIndicesEXT[prim_idx] = prim;
    }

    // Generate vertices
    [[unroll]]
    for (uint i = 0; i < ITERS_PER_VERT; i++) {
        // Fetch vertex index and break early if OOB
        const uint vert_idx = (MS_INVOCATIONS * i) + gl_LocalInvocationIndex;
        if (vert_idx >= vert_count) {
            continue;
        }

        /*
        // Colors
        const vec3 COLORS[7] = {
            vec3(1.0, 0.0, 0.0),
            vec3(0.0, 1.0, 0.0),
            vec3(0.0, 0.0, 1.0),
            vec3(1.0, 1.0, 0.0),
            vec3(1.0, 0.0, 1.0),
            vec3(0.0, 1.0, 1.0),
            vec3(1.0, 1.0, 1.0)
        };

        vs_Color[vert_idx] = COLORS[index_offset % 7];
        */

        // Read attributes
        const vec4 ard_position = v_positions[vertex_offset + vert_idx];
#ifdef COLOR_PASS
        const uvec2 ard_normal_raw = v_normals[vertex_offset + vert_idx];
#if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0    
        const uvec2 ard_tangent_raw = v_tangents[vertex_offset + vert_idx];
#endif
#endif
        
#if ARD_VS_HAS_UV0
        const uint ard_uv0_raw = v_uv0s[vertex_offset + vert_idx];
#endif

        // Format conversion
#ifdef COLOR_PASS
        vec4 ard_normal = vec4(
            unpackSnorm2x16(ard_normal_raw.x),
            unpackSnorm2x16(ard_normal_raw.y)
        );
        ard_normal = normalize(ard_normal);
#if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0    
        vec4 ard_tangent = vec4(
            unpackSnorm2x16(ard_tangent_raw.x),
            unpackSnorm2x16(ard_tangent_raw.y)
        );
        ard_tangent = normalize(ard_tangent);
#endif
#endif
        
#if ARD_VS_HAS_UV0
        const vec2 ard_uv0 = unpackHalf2x16(ard_uv0_raw);
#endif
        const vec4 ws_frag_pos = model * ard_position;
        const vec4 position = camera[gl_ViewIndex].vp * ws_frag_pos;
#ifdef COLOR_PASS
        vs_ViewSpacePosition[vert_idx] = camera[gl_ViewIndex].view * ws_frag_pos;
#endif

        gl_MeshVerticesEXT[vert_idx].gl_Position = position;
#ifdef COLOR_PASS
        vs_Position[vert_idx] = position;
        vs_WorldSpaceFragPos[vert_idx] = ws_frag_pos.xyz;
#endif

        // Compute TBN if we have tangents and UVs (UVs are required as well because the TBN is 
        // only used when doing normal mapping.
#ifdef COLOR_PASS
#if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0
        vec3 T = normalize(vec3(model * ard_tangent));
        vec3 N = normalize(vec3(model * ard_normal));
        T = normalize(T - dot(T, N) * N);
        vec3 B = cross(N, T);
        
        vs_TBN[vert_idx] = mat3(T, B, N);
#endif
        // Output corrected normal
        vs_Normal[vert_idx] = normalize(normal_mat * ard_normal.xyz);
#endif

#if ARD_VS_HAS_UV0
        vs_Uv[vert_idx] = ard_uv0;
#if ARD_VS_HAS_TANGENT
        vs_Slots[vert_idx] = uvec4(color_slot, met_rough_slot, normal_slot, materials_slot);
#else
        vs_Slots[vert_idx] = uvec4(color_slot, met_rough_slot, EMPTY_TEXTURE_ID, materials_slot);
#endif
#else
        vs_Slots[vert_idx] = uvec4(EMPTY_TEXTURE_ID, EMPTY_TEXTURE_ID, EMPTY_TEXTURE_ID, materials_slot);
#endif
    }
}