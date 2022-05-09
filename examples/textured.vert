// Compile with:
// glslc textured.vert -o textured.vert.spv
#version 450 core

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec2 UV0;

layout(location = 0) out vec2 OUT_UV0;
layout(location = 1) flat out uint INSTANCE_IDX;

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
    vec4[6] frustum;
    vec4 properties;
    vec4 position;
} camera;

void main() {
    INSTANCE_IDX = gl_InstanceIndex;
    OUT_UV0 = UV0;
    gl_Position = camera.vp * objects[obj_idxs[gl_InstanceIndex]].model * vec4(POSITION.xyz, 1.0);
}