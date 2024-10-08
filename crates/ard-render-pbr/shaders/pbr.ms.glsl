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

shared mat4x3 s_model_mat;
#if defined(COLOR_PASS)
shared mat4x3 s_last_model_mat;
#endif
shared mat3 s_normal_mat;
shared uint s_object_id;
shared uint s_vertex_offset;
shared uint s_index_offset;
shared uint s_vert_prim_counts;

#if defined(ENTITY_PASS)
shared uint s_entity;
#endif
#if ARD_VS_HAS_UV0
shared uint s_color_slot;
shared uint s_met_rough_slot;
#if ARD_VS_HAS_TANGENT
shared uint s_normal_slot;
#endif
#endif

void main() {
    // Read in everything for the workgroup.
    if (gl_LocalInvocationIndex == 0) {
        // Meshlet info
        const uint meshlet_id = uint(output_ids[payload.meshlet_base + gl_WorkGroupID.x]);
        const uvec3 meshlet = v_meshlets[payload.meshlet_info_base + meshlet_id].data.xyz;

        s_vertex_offset = meshlet.x;
        s_index_offset = meshlet.y;
        s_vert_prim_counts = meshlet.z;

        // Shading properties
        s_model_mat = payload.model;
#if defined(COLOR_PASS)
        s_last_model_mat = payload.last_model;
#endif
        s_normal_mat = payload.normal;
        s_object_id = payload.object_id;
#if defined(ENTITY_PASS)
        s_entity = payload.entity;
#endif
#if ARD_VS_HAS_UV0
        s_color_slot = payload.color_tex;
        s_met_rough_slot = payload.met_rough_tex;
#if ARD_VS_HAS_TANGENT
        s_normal_slot = payload.normal_tex;
#endif
#endif
    }

    barrier();

    // Extract shared values
    const mat4x3 model = s_model_mat;
#if defined(COLOR_PASS)
    const mat4x3 last_model = s_last_model_mat;
#endif
    const mat3 normal_mat = s_normal_mat;
    const uint object_id = s_object_id;
    const uint index_offset = s_index_offset;
    const uint vertex_offset = s_vertex_offset;
    const uint vp_count = s_vert_prim_counts & 0xFFFF;
    const uint vert_count = vp_count & 0xFF;
    const uint prim_count = (vp_count >> 8) & 0xFF;

#if defined(ENTITY_PASS)
    const uint entity = s_entity;
#endif
#if ARD_VS_HAS_UV0
    const uint color_slot = s_color_slot;
    const uint met_rough_slot = s_met_rough_slot;
#if ARD_VS_HAS_TANGENT
    const uint normal_slot = s_normal_slot;
#endif
#endif

    // Allocate outputs
    if (gl_LocalInvocationIndex == 0) {
        SetMeshOutputsEXT(vert_count, prim_count);
    }

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
        gl_PrimitiveTriangleIndicesEXT[prim_idx] = uvec3(
            v_indices[base] - vertex_offset,
            v_indices[base + 1] - vertex_offset,
            v_indices[base + 2] - vertex_offset
        );
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
        const vec4 COLORS[7] = {
            vec4(1.0, 0.0, 0.0, 1.0),
            vec4(0.0, 1.0, 0.0, 1.0),
            vec4(0.0, 0.0, 1.0, 1.0),
            vec4(1.0, 1.0, 0.0, 1.0),
            vec4(1.0, 0.0, 1.0, 1.0),
            vec4(0.0, 1.0, 1.0, 1.0),
            vec4(1.0, 1.0, 1.0, 1.0)
        };

        vs_out[vert_idx].color = COLORS[index_offset % 7];
        */

        // Read attributes
        const vec4 ard_position = v_positions[vertex_offset + vert_idx];
        const uvec2 ard_normal_raw = v_normals[vertex_offset + vert_idx];
#if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0    
        const uvec2 ard_tangent_raw = v_tangents[vertex_offset + vert_idx];
#endif
        
#if ARD_VS_HAS_UV0
        const uint ard_uv0_raw = v_uv0s[vertex_offset + vert_idx];
#endif

        // Format conversion
        vec4 ard_normal = vec4(
            unpackSnorm2x16(ard_normal_raw.x),
            unpackSnorm2x16(ard_normal_raw.y)
        );
#if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0    
        vec4 ard_tangent = vec4(
            unpackSnorm2x16(ard_tangent_raw.x),
            unpackSnorm2x16(ard_tangent_raw.y)
        );
#endif
        
#if defined(ENTITY_PASS)
        vs_out[vert_idx].entity = entity;
#endif

#if ARD_VS_HAS_UV0
        const vec2 ard_uv0 = unpackHalf2x16(ard_uv0_raw);
#endif
        const vec4 ws_frag_pos = vec4(model * ard_position, 1.0);
        const vec4 position = camera[gl_ViewIndex].vp * ws_frag_pos;
#ifdef COLOR_PASS
        const vec4 ws_last_frag_pos = vec4(last_model * ard_position, 1.0);
        const vec4 last_position = camera[gl_ViewIndex].last_vp * ws_last_frag_pos;
        vs_out[vert_idx].view_space_position = camera[gl_ViewIndex].view * ws_frag_pos;
#endif

        gl_MeshVerticesEXT[vert_idx].gl_Position = position;
#ifdef COLOR_PASS
        vs_out[vert_idx].ndc_position = position;
        vs_out[vert_idx].ndc_last_position = last_position;
#endif
        vs_out[vert_idx].world_space_position = ws_frag_pos.xyz;

        // Send tangents if we have them and UVs (UVs are required as well because the TBN is 
        // only used when doing normal mapping).
#if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0
        vec3 tangentW = normalize(normal_mat * ard_tangent.xyz);
        vec3 normalW = normalize(normal_mat * ard_normal.xyz);
        tangentW = normalize(tangentW - dot(tangentW, normalW) * normalW);
        vec3 bitangentW = normalize(cross(normalW, tangentW) * ard_tangent.w);

        vs_out[vert_idx].tangent = tangentW;
        vs_out[vert_idx].bitangent = bitangentW;
        vs_out[vert_idx].normal = normalW;
#else
        vs_out[vert_idx].normal = normalize(normal_mat * ard_normal.xyz);
#endif

#if ARD_VS_HAS_UV0
        vs_out[vert_idx].uv = ard_uv0;
#if ARD_VS_HAS_TANGENT
        vs_out[vert_idx].slots = uvec4(color_slot, met_rough_slot, normal_slot, object_id);
#else
        vs_out[vert_idx].slots = uvec4(color_slot, met_rough_slot, EMPTY_TEXTURE_ID, object_id);
#endif
#else
        vs_out[vert_idx].slots = uvec4(EMPTY_TEXTURE_ID, EMPTY_TEXTURE_ID, EMPTY_TEXTURE_ID, object_id);
#endif
    }
}