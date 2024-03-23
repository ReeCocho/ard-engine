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
BoundingBox transform_bounding_box(mat4 model, vec3 min_pt, vec3 max_pt) {
    BoundingBox bb;

    // Compute all eight corners
    const mat4 proj = camera[0].projection;
    const mat4 vm = camera[0].view * model;
    bb.corners[0] = vm * vec4(min_pt.x, min_pt.y, min_pt.z, 1.0);
    bb.corners[1] = vm * vec4(max_pt.x, min_pt.y, min_pt.z, 1.0);
    bb.corners[2] = vm * vec4(min_pt.x, max_pt.y, min_pt.z, 1.0);
    bb.corners[3] = vm * vec4(min_pt.x, min_pt.y, max_pt.z, 1.0);
    bb.corners[4] = vm * vec4(max_pt.x, max_pt.y, min_pt.z, 1.0);
    bb.corners[5] = vm * vec4(min_pt.x, max_pt.y, max_pt.z, 1.0);
    bb.corners[6] = vm * vec4(max_pt.x, min_pt.y, max_pt.z, 1.0);
    bb.corners[7] = vm * vec4(max_pt.x, max_pt.y, max_pt.z, 1.0);

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

void main() {
    // Stop if we're OOB
    if (gl_GlobalInvocationID.x >= consts.object_id_count) {
        return;
    }

    // Fetch data to send to mesh shader
    const uint object_idx = consts.object_id_offset + gl_GlobalInvocationID.x;
    const ObjectId id = object_ids[object_idx];
    const uint data_idx = id.data_idx & 0x7FFFFFFF;
    const Meshlet meshlet = v_meshlets[id.meshlet];

    // Extract properties from the meshlet
    const uint vertex_offset = meshlet.data.x;
    const uint index_offset = meshlet.data.y;
    const uint vertex_prim_counts = meshlet.data.z & 0xFFFF;

    // If this is the depth prepass or a shader pass we perform culling.
#if defined(DEPTH_PREPASS) || defined(SHADOW_PASS)
    bool visible = true;

    const mat4 model_mat = object_data[data_idx].model;
    const uint mesh_id = object_data[data_idx].mesh;
    const ObjectBounds obj_bounds = mesh_info[mesh_id].bounds;
    const vec3 scale = vec3(
        dot(model_mat[0].xyz, model_mat[0].xyz),
        dot(model_mat[1].xyz, model_mat[1].xyz),
        dot(model_mat[2].xyz, model_mat[2].xyz)
    );

    // Extract relative min and max AABB points
    const vec3 bounds_range = obj_bounds.max_pt.xyz - obj_bounds.min_pt.xyz;
    const vec4 zunpacked = unpackUnorm4x8(meshlet.data.z);
    const vec4 wunpacked = unpackUnorm4x8(meshlet.data.w);
    const vec3 meshlet_min_pt = obj_bounds.min_pt.xyz 
        + (bounds_range * vec3(zunpacked.z, zunpacked.w, wunpacked.x));
    const vec3 meshlet_max_pt = obj_bounds.min_pt.xyz 
        + (bounds_range * vec3(wunpacked.y, wunpacked.z, wunpacked.w));

    // Frustum culling based on bounding sphere.
    vec3 meshlet_center = (meshlet_max_pt + meshlet_min_pt) * 0.5;
    const float meshlet_radius = -length(meshlet_max_pt - meshlet_center) 
        * sqrt(max(scale.x, max(scale.y, scale.z)));
    meshlet_center = (model_mat * vec4(meshlet_center, 1.0)).xyz;

    // NOTE: We are only checking the first five planes because we're using an infinite perspective
    // matrix, meaning all objects are always within the final plane. If that should ever change,
    // make sure to add back in the check for the final plane.
    [[unroll]]
    for (int i = 0; i < 5; i++) { 
        const float dist = dot(vec4(meshlet_center, 1.0), camera[0].frustum.planes[i]);
        if (dist < meshlet_radius) {
            visible = false;
            break;
        }
    }

    // Perform occlusion culling in the depth prepass only
#if defined(DEPTH_PREPASS)
    if (visible) {
        BoundingBox bb = transform_bounding_box(
            model_mat,
            meshlet_min_pt,
            meshlet_max_pt
        );

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
        visible = bb.depth <= depth;
    }
#endif

    // If this is a shadow pass, we don't actually need to write back the object ID since we don't
    // reused culling results between frames.
#if !defined(SHADOW_PASS)
    // Write visibility flag into object ID
    uint new_data_idx = data_idx;
    if (visible) {
        new_data_idx |= 1 << 31;
    } 
    object_ids[object_idx].data_idx = new_data_idx;
#endif
#else
    // Otherwise, we extract the visibility value from the data index
    bool visible = (id.data_idx >> 31) > 0;
#endif

    // Vote on visibility
    uvec4 valid_votes = subgroupBallot(visible);

    // If we are visible, write our data into the payload
    if (visible) {
        const uint index = subgroupBallotExclusiveBitCount(valid_votes);
        payload.object_ids[index] = data_idx;
        payload.index_offsets[index] = index_offset;
        payload.vertex_offsets[index] = vertex_offset;
        payload.counts[index] = vertex_prim_counts;
    }
    
    // Generate mesh tasks if we are ID 0
    if (gl_LocalInvocationIndex == 0) {
        const uint meshlet_count = subgroupBallotBitCount(valid_votes);
        EmitMeshTasksEXT(meshlet_count, 1, 1);
    }
}