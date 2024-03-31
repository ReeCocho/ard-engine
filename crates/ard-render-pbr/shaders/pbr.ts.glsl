#version 450 core
#pragma shader_stage(task)
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_multiview : enable
#extension GL_KHR_shader_subgroup_ballot : require
#extension GL_EXT_mesh_shader: require
#extension GL_EXT_control_flow_attributes : enable

#define ARD_TEXTURE_COUNT 3
#define TASK_SHADER
#define ArdMaterialData PbrMaterial
#include "pbr_common.glsl"
#include "utils.glsl"

layout(push_constant) uniform constants {
    DrawPushConstants consts;
};

struct BoundingBox {
    /// All eight corners of the box in world space.
    vec4[8] corners;
    /// Min point for AABB in screen space.
    vec2 min_pt;
    /// Max point for AABB in screen space.
    vec2 max_pt;
    /// Depth value for the AABB square in world space.
    float depth;
};

/// Transforms the bounding box via the given model matrix.
BoundingBox transform_bounding_box(mat4 view_model, vec3 min_pt, vec3 max_pt) {
    BoundingBox bb;

    // Compute all eight corners
    const mat4 proj = camera[0].projection;
    bb.corners[0] = view_model * vec4(min_pt.x, min_pt.y, min_pt.z, 1.0);
    bb.corners[1] = view_model * vec4(max_pt.x, min_pt.y, min_pt.z, 1.0);
    bb.corners[2] = view_model * vec4(min_pt.x, max_pt.y, min_pt.z, 1.0);
    bb.corners[3] = view_model * vec4(min_pt.x, min_pt.y, max_pt.z, 1.0);
    bb.corners[4] = view_model * vec4(max_pt.x, max_pt.y, min_pt.z, 1.0);
    bb.corners[5] = view_model * vec4(min_pt.x, max_pt.y, max_pt.z, 1.0);
    bb.corners[6] = view_model * vec4(max_pt.x, min_pt.y, max_pt.z, 1.0);
    bb.corners[7] = view_model * vec4(max_pt.x, max_pt.y, max_pt.z, 1.0);

    // Then find the min and max points in screen space
    bb.min_pt = vec2(uintBitsToFloat(0x7F800000));
    bb.max_pt = vec2(uintBitsToFloat(0xFF800000));
    bb.depth = uintBitsToFloat(0x7F800000);

    [[unroll]]
    for (int i = 0; i < 8; i++) {
        // Depth
        bb.depth = min(bb.depth, bb.corners[i].z);
    
        bb.corners[i] = proj * bb.corners[i];
        bb.corners[i] /= bb.corners[i].w;
        vec4 pt = bb.corners[i];

        bb.min_pt = min(bb.min_pt, pt.xy);
        bb.max_pt = max(bb.max_pt, pt.xy);
    }

    return bb;
}

bool is_visible(vec3 center, float radius, vec3 min_pt, vec3 max_pt, mat4 view_model) {
    // NOTE: We are only checking the first five planes because we're using an infinite perspective
    // matrix, meaning all objects are always within the final plane. If that should ever change,
    // make sure to add back in the check for the final plane.
    [[unroll]]
    for (int i = 0; i < 5; i++) { 
        const float dist = dot(vec4(center, 1.0), camera[0].frustum.planes[i]);
        if (dist < radius) {
            return false;
        }
    }

    // Only the depth prepass and transparent pass have occlusion culling.
#if defined(DEPTH_PREPASS) || defined(TRANSPARENT_PASS)
    BoundingBox bb = transform_bounding_box(view_model, min_pt, max_pt);

    // Determine the appropriate mip level to sample the HZB image
    vec2 dbl_pixel_size = vec2(
        bb.max_pt.x - bb.min_pt.x,
        bb.max_pt.y - bb.min_pt.y
    ) * consts.render_area;
    float level = floor(log2(max(dbl_pixel_size.x, dbl_pixel_size.y) * 0.5));

    // Clip space to UV
    bb.max_pt = (bb.max_pt * 0.5) + vec2(0.5);
    bb.max_pt.y = 1.0 - bb.max_pt.y;

    bb.min_pt = (bb.min_pt * 0.5) + vec2(0.5);
    bb.min_pt.y = 1.0 - bb.min_pt.y;

    // Determine depth (in world space)
    float depth = textureLod(hzb_image, (bb.max_pt + bb.min_pt) * 0.5, level).x;
    depth = camera[0].near_clip / depth;

    // Check for visibility
    return bb.depth <= depth;
#else
    return true;
#endif
}

void manual_payload(const ObjectId id) {
    const uint textures_slot = object_data[id.data_idx].textures;
    payload.meshlet_base = 1 + id.meshlet_base;
    payload.meshlet_info_base = mesh_info[object_data[id.data_idx].mesh].meshlet_offset;
    payload.model = transpose(object_data[id.data_idx].model);
    payload.normal = mat3(transpose(object_data[id.data_idx].model_inv));
    payload.object_id = id.data_idx;
#if ARD_VS_HAS_UV0
    payload.color_tex = uint(texture_slots[textures_slot][0]);
    payload.met_rough_tex = uint(texture_slots[textures_slot][2]);
#if ARD_VS_HAS_TANGENT
    payload.normal_tex = uint(texture_slots[textures_slot][1]);
#endif
#endif
}

// Shared variables used by all invocations when culling is required.
#if defined(DEPTH_PREPASS) || defined(SHADOW_PASS) || defined(TRANSPARENT_PASS)
    shared bool s_visible;
    shared mat4x3 s_model_mat;
    shared mat3 s_normal_mat;
    shared mat4 s_view_model;
    shared ObjectBounds s_obj_bounds;
    shared uint s_data_idx;
    shared uint s_meshlet_offset;
    shared uint s_meshlet_count;
    shared uint s_output_base;
    shared float s_max_scale_axis;
#if ARD_VS_HAS_UV0
    shared uint s_color_tex;
    shared uint s_met_rough_tex;
#if ARD_VS_HAS_TANGENT
    shared uint s_normal_tex;
#endif
#endif
#endif

void main() {
    // Stop if we're OOB.
    if (gl_WorkGroupID.x >= consts.object_id_count) {
        return;
    }

    const uint object_idx = consts.object_id_offset + gl_WorkGroupID.x;

    // If this is the depth prepass or shadow shaders, we need to decide visibility ourselves.
#if defined(DEPTH_PREPASS) || defined(SHADOW_PASS) || defined(TRANSPARENT_PASS)

    // Check for culling lock.
    if (consts.lock_culling == 1) {
        if (gl_LocalInvocationIndex == 0) {
            const ObjectId id = input_ids[object_idx];
            const uint meshlet_count = uint(output_ids[id.meshlet_base]);
            manual_payload(id);
            EmitMeshTasksEXT(meshlet_count, 1, 1);
        }
        return;
    }

    // Invocation 0 does whole object culling first.
    if (gl_LocalInvocationIndex == 0) {
        // Read in mesh information
        const ObjectId id = input_ids[object_idx];
        const mat4x3 model_mat = transpose(object_data[id.data_idx].model);
        const mat3 normal_mat = mat3(transpose(object_data[id.data_idx].model_inv));
        const uint textures_slot = object_data[id.data_idx].textures;
        const uint mesh_id = object_data[id.data_idx].mesh;
        const uint meshlet_offset = mesh_info[mesh_id].meshlet_offset;
        const uint meshlet_count = mesh_info[mesh_id].meshlet_count;
        const mat4 view_model = camera[0].view * mat4(
            vec4(model_mat[0], 0.0),
            vec4(model_mat[1], 0.0),
            vec4(model_mat[2], 0.0),
            vec4(model_mat[3], 1.0)
        );

        // Compute bounds
        const ObjectBounds obj_bounds = mesh_info[mesh_id].bounds;
        const vec3 scale = vec3(
            dot(model_mat[0].xyz, model_mat[0].xyz),
            dot(model_mat[1].xyz, model_mat[1].xyz),
            dot(model_mat[2].xyz, model_mat[2].xyz)
        );

        // Figure out required bounds for culling
        const vec3 bounds_range = obj_bounds.max_pt.xyz - obj_bounds.min_pt.xyz;
        const float max_scale_axis = sqrt(max(scale.x, max(scale.y, scale.z)));
        vec3 obj_center = (obj_bounds.max_pt.xyz + obj_bounds.min_pt.xyz) * 0.5;
        const float obj_radius = 
            (-max_scale_axis * length(obj_bounds.max_pt.xyz - obj_center)) - 0.05;
        obj_center = (model_mat * vec4(obj_center, 1.0)).xyz;

        // Do culling
        s_visible = is_visible(
            obj_center, 
            obj_radius, 
            obj_bounds.min_pt.xyz, 
            obj_bounds.max_pt.xyz, 
            view_model
        );

        // If we aren't visible, write out that we have 0 meshlets
        if (!s_visible) {
            output_ids[id.meshlet_base] = uint16_t(0);
        }

        // Write shared variables
        s_model_mat = model_mat;
        s_normal_mat = normal_mat;
        s_view_model = view_model;
        s_output_base = id.meshlet_base;
        s_obj_bounds = obj_bounds;
        s_data_idx = id.data_idx;
        s_meshlet_offset = meshlet_offset;
        s_meshlet_count = meshlet_count;
        s_max_scale_axis = max_scale_axis;
#if ARD_VS_HAS_UV0
        s_color_tex = texture_slots[textures_slot][0];
        s_met_rough_tex = texture_slots[textures_slot][2];
#if ARD_VS_HAS_TANGENT
        s_normal_tex = texture_slots[textures_slot][1];
#endif
#endif
    }

    barrier();
    
    // If we are not visible, stop early
    if (!s_visible) {
        return;
    }

    // Read in shared variables
    const mat4x3 model_mat = s_model_mat;
    const mat4 view_model = s_view_model;
    const ObjectBounds obj_bounds = s_obj_bounds;
    const uint meshlet_offset = s_meshlet_offset;
    const uint meshlet_count = s_meshlet_count;
    const uint output_base = s_output_base;
    const float max_scale_axis = s_max_scale_axis;
    const vec3 bounds_range = obj_bounds.max_pt.xyz - obj_bounds.min_pt.xyz;

    uint output_meshlet_count = 0;

    // Distribute meshlets over all invocations
    const uint iters = (meshlet_count + (TS_INVOCATIONS - 1)) / TS_INVOCATIONS;
    for (uint i = 0; i < iters; i++) {
        // Determine meshlet index. Skip if OOB.
        const uint meshlet_idx = (i * TS_INVOCATIONS) + gl_LocalInvocationIndex;
        if (meshlet_idx >= meshlet_count) {
            continue;
        }

        // Read in the meshlet this invocation is looking at.
        const uvec2 bounds_packed = v_meshlets[meshlet_offset + meshlet_idx].data.zw;

        // Unpack meshlet bounds.
        const vec4 xunpacked = unpackUnorm4x8(bounds_packed.x);
        const vec4 yunpacked = unpackUnorm4x8(bounds_packed.y);
        const vec3 meshlet_min_pt = obj_bounds.min_pt.xyz 
            + (bounds_range * vec3(xunpacked.z, xunpacked.w, yunpacked.x));
        const vec3 meshlet_max_pt = obj_bounds.min_pt.xyz 
            + (bounds_range * vec3(yunpacked.y, yunpacked.z, yunpacked.w));

        // Compute meshlet bounds
        vec3 meshlet_center = (meshlet_max_pt + meshlet_min_pt) * 0.5;
        const float meshlet_radius = 
            (-max_scale_axis * length(meshlet_max_pt - meshlet_center)) - 0.05;
        meshlet_center = model_mat * vec4(meshlet_center, 1.0);

        // Perform culling
        const bool visible = is_visible(
            meshlet_center, 
            meshlet_radius, 
            meshlet_min_pt, 
            meshlet_max_pt, 
            view_model
        );
        
        // Vote on visibility
        uvec4 valid_votes = subgroupBallot(visible);
        const uint visible_count = subgroupBallotBitCount(valid_votes);
        const uint out_idx = subgroupBallotExclusiveBitCount(valid_votes);

        // Write out meshlet ID if visible
        if (visible) {
            output_ids[1 + output_base + output_meshlet_count + out_idx] = uint16_t(meshlet_idx);
        }

        // Update output counter
        output_meshlet_count += visible_count;
    }

    // Write to payload and emit tasks
    if (gl_LocalInvocationIndex == 0) {
        output_ids[output_base] = uint16_t(output_meshlet_count);

        payload.meshlet_base = 1 + output_base;
        payload.meshlet_info_base = meshlet_offset;  
        payload.model = model_mat;
        payload.normal = s_normal_mat;
        payload.object_id = s_data_idx;
#if ARD_VS_HAS_UV0
        payload.color_tex = s_color_tex;
        payload.met_rough_tex = s_met_rough_tex;
#if ARD_VS_HAS_TANGENT
        payload.normal_tex = s_normal_tex;
#endif
#endif

        EmitMeshTasksEXT(output_meshlet_count, 1, 1);
    }

#else
    // Otherwise, we just do a simple lookup to see how many meshlets are visible
    if (gl_LocalInvocationIndex == 0) {
        const ObjectId id = input_ids[object_idx];
        const uint meshlet_count = uint(output_ids[id.meshlet_base]);
        manual_payload(id);
        EmitMeshTasksEXT(meshlet_count, 1, 1);
    }
#endif
}
