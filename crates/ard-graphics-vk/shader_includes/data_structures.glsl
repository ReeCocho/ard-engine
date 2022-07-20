#ifndef _DATA_STRUCTURES_GLSL
#define _DATA_STRUCTURES_GLSL

/// Contains commonly used data structures.

#include "constants.glsl"

////////////////
/// LIGHTING ///
////////////////

struct ShadowCascadeInfo {
    mat4 vp;
    mat4 view;
    mat4 proj;
    vec2 uv_size;
    float far_plane;
    float min_bias;
    float max_bias;
    float depth_range;
};

struct Lighting {
    ShadowCascadeInfo cascades[MAX_SHADOW_CASCADES];
    vec4 ambient;
    vec4 sun_color_intensity;
    vec4 sun_direction;
    float sun_size;
    uint radiance_mip_count;
};

struct PointLight {
    vec4 color_intensity;
    vec4 position_range;
};

struct PointLightTable {
    int[FROXEL_TABLE_Z][FROXEL_TABLE_X][FROXEL_TABLE_Y] light_counts;
    uint[FROXEL_TABLE_Z][FROXEL_TABLE_X][FROXEL_TABLE_Y][MAX_POINT_LIGHTS_PER_FROXEL] clusters;
};

//////////////
/// CAMERA ///
//////////////

struct Froxel {
    vec4[4] planes;
    vec4 min_max_z;
};

struct CameraClusterFroxels {
    Froxel froxels[FROXEL_TABLE_Z][FROXEL_TABLE_X][FROXEL_TABLE_Y];
};

struct Camera {
    mat4 view;
    mat4 projection;
    mat4 vp;
    mat4 view_inv;
    mat4 projection_inv;
    mat4 vp_inv;
    vec4[6] planes;
    vec4 position;
    vec2 scale_bias;
    float fov;
    float near_clip;
    float far_clip;
};

///////////////////////
/// DRAW GENERATION ///
///////////////////////

struct ObjectId {
    uint info_idx;
    uint batch_idx;
    uint dummy;
};

struct ObjectInfo {
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

////////////
/// MISC ///
////////////

struct VsOut {
    vec3 frag_pos;
};

struct TransformedBoundingBox {
    /// All eight corners of the box
    vec4[8] corners;
    /// Min point for AABB in screen space.
    vec2 min_pt;
    /// Max point for AABB in screen space.
    vec2 max_pt;
    /// Depth value for the AABB square in world space.
    float depth;
};

#endif