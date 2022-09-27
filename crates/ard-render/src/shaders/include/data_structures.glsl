#ifndef _DATA_STRUCTURES_GLSL
#define _DATA_STRUCTURES_GLSL

#include "constants.glsl"

// NOTE: See /src/renderer/render_data.rs for the definitions of most of these structures. If
// you're experiencing weird data corruption, odds are that one of those structures was modified
// and then not updated here. Might be worth investigating a way to auto-generate this file from
// the Rust files.

////////////////
/// LIGHTING ///
////////////////

struct Light {
    vec4 color_intensity;
    vec4 position_range;
    // Unused by point lights.
    vec4 direction_radius;
};

struct LightClusters {
    uint[FROXEL_TABLE_Z][FROXEL_TABLE_X][FROXEL_TABLE_Y] light_counts;
    uint[FROXEL_TABLE_Z][FROXEL_TABLE_X][FROXEL_TABLE_Y][MAX_LIGHTS_PER_FROXEL] clusters;
};

//////////////
/// CAMERA ///
//////////////

struct Froxel {
    vec4[4] planes;
    vec4 min_max_z;
};

struct Frustum {
    vec4[6] planes;
};

struct CameraFroxels {
    Froxel froxels[FROXEL_TABLE_Z][FROXEL_TABLE_X][FROXEL_TABLE_Y];
};

struct Camera {
    mat4 view;
    mat4 projection;
    mat4 vp;
    mat4 view_inv;
    mat4 projection_inv;
    mat4 vp_inv;
    Frustum frustum;
    vec4 position;
    vec2 cluster_scale_bias;
    float fov;
    float near_clip;
    float far_clip;
};

///////////////////////
/// DRAW GENERATION ///
///////////////////////

struct ObjectData {
    mat4 model;
    uint material;
    uint textures;
    uint entity_id;
    uint entity_ver;
};

struct DrawCall {
    uint index_count;
    uint instance_count;
    uint first_index;
    int vertex_offset;
    uint first_instance;
    vec4 bounds_center;
    vec4 bounds_half_extents;
};

struct ObjectId {
    uint draw_idx;
    uint padding1;
    uint data_idx;
    uint padding2;
};

////////////
/// MISC ///
////////////

struct VsOut {
    /// The fragment position in world space.
    vec3 frag_pos;
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

#endif