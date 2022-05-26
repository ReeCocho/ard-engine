// Compile with:
// glslc triangle.frag -o triangle.frag.spv
#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 VERT_COLOR;

struct Material {
    vec4 color;
};

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

layout(set = 3, binding = 1) readonly buffer MaterialData {
    Material[] materials;
};

layout(location = 1) flat in uint INSTANCE_IDX;

void main() {
    FRAGMENT_COLOR = materials[objects[obj_idxs[INSTANCE_IDX]].material].color;
}