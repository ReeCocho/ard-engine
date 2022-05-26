// Compile with:
// glslc textured.frag -o textured.frag.spv
#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec2 IN_UV0;
layout(location = 1) flat in uint INSTANCE_IDX;

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

layout(set = 1, binding = 0) uniform sampler2D[] TEXTURES;

layout(set = 3, binding = 0) readonly buffer TextureData {
    uint[][8] material_textures;
};

void main() {
    uint obj_idx = obj_idxs[INSTANCE_IDX];
    uint material_textures_idx = objects[obj_idx].textures;
    uint texture_idx = material_textures[material_textures_idx][0];
    FRAGMENT_COLOR = texture(TEXTURES[0], IN_UV0);
}