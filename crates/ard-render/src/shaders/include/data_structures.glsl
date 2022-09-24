#ifndef _DATA_STRUCTURES_GLSL
#define _DATA_STRUCTURES_GLSL

// NOTE: See /src/renderer/render_data.rs for the definitions of most of these structures. If
// you're experiencing weird data corruption, odds are that one of those structures was modified
// and then not updated here. Might be worth investigating a way to auto-generate this file from
// the Rust files.

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

#endif