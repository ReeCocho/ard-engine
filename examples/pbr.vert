#version 450 core

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 NORMAL;
layout(location = 2) in vec4 TANGENT;
layout(location = 3) in vec4 COLOR;

layout(location = 0) out vec4 FRAG_POS;
layout(location = 1) out vec4 OUT_COLOR;
layout(location = 2) out vec3 WORLD_POS;
layout(location = 4) flat out uint INSTANCE_IDX;

struct ObjectInfo {
    mat4 model;
    uint material;
    uint textures;
};

layout(set = 0, binding = 0) readonly buffer InputInfoData {
    ObjectInfo[] objects;
};

layout(set = 0, binding = 2) readonly buffer InputObjectIdxs {
    uint[] obj_idxs;
};

layout(set = 2, binding = 0) uniform CameraUBO {
    mat4 view;
    mat4 projection;
    mat4 vp;
    mat4 view_inv;
    mat4 projection_inv;
    mat4 vp_inv;
    vec4[6] planes;
    vec4 properties;
    vec4 position;
    vec2 scale_bias;
} camera;

void main() {
    INSTANCE_IDX = gl_InstanceIndex;
    vec4 world_pos = objects[obj_idxs[gl_InstanceIndex]].model * vec4(POSITION.xyz, 1.0);
    gl_Position = camera.vp * world_pos;
    WORLD_POS = world_pos.xyz;
    FRAG_POS = gl_Position;
    OUT_COLOR = COLOR;
}