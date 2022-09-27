#ifndef _ARD_STD_GLSL
#define _ARD_STD_GLSL

#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

#include "constants.glsl"
#include "data_structures.glsl"

const float PI = 3.14159265359;

#define GLOBAL_SET_ID 0
#define CAMERA_SET_ID 1
#define TEXTURES_SET_ID 2
#define MATERIALS_SET_ID 3

//////////////////////////////////////////
/// Required shader inputs and outputs ///
//////////////////////////////////////////

#ifdef ARD_VERTEX_SHADER
layout(location = 16) flat out uint ARD_INSTANCE_IDX;
layout(location = 17) flat out uint ARD_MATERIAL_IDX;
layout(location = 18) flat out uint ARD_TEXTURES_IDX;
layout(location = 19) out vec3 ARD_FRAG_POS;
layout(location = 20) out vec3 ARD_FRAG_POS_VIEW_SPACE;
#endif

#ifdef ARD_FRAGMENT_SHADER
layout(location = 16) flat in uint ARD_INSTANCE_IDX;
layout(location = 17) flat in uint ARD_MATERIAL_IDX;
layout(location = 18) flat in uint ARD_TEXTURES_IDX;
layout(location = 19) in vec3 ARD_FRAG_POS;
layout(location = 20) in vec3 ARD_FRAG_POS_VIEW_SPACE;
#endif

//////////////////
/// GLOBAL SET ///
//////////////////

layout(set = GLOBAL_SET_ID, binding = 0) restrict readonly buffer ARD_ObjectData {
    ObjectData[] ARD_OBJECT_DATA;
};

layout(set = GLOBAL_SET_ID, binding = 1) restrict readonly buffer ARD_ObjectIndices {
    uint[] ARD_OBJECT_INDICES;
};

layout(set = GLOBAL_SET_ID, binding = 2) restrict readonly buffer ARD_Lights {
    Light[] ARD_LIGHTS;
};

layout(set = GLOBAL_SET_ID, binding = 3) restrict readonly buffer ARD_Clusters {
    LightClusters ARD_CLUSTERS;
};

//////////////
/// CAMERA ///
//////////////

layout(set = CAMERA_SET_ID, binding = 0) uniform ARD_Camera {
    Camera camera;
};

////////////////
/// TEXTURES ///
////////////////

layout(set = TEXTURES_SET_ID, binding = 0) uniform sampler2D[] ARD_TEXTURES;

/////////////////
/// MATERIALS ///
/////////////////

#ifdef ARD_TEXTURE_COUNT
layout(set = MATERIALS_SET_ID, binding = 0) readonly buffer ARD_TextureData {
    uint[][MAX_TEXTURES_PER_MATERIAL] ARD_MATERIAL_TEXTURES;
};
#endif

#ifdef ARD_MATERIAL
layout(set = MATERIALS_SET_ID, binding = 1) readonly buffer ARD_MaterialData {
    ARD_MATERIAL[] ARD_MATERIALS;
};
#endif

////////////////////
/// ENTRY POINTS ///
////////////////////

#ifdef ARD_VERTEX_SHADER

#define ARD_ENTRY(func) \
void main() { \
    uint idx = ARD_OBJECT_INDICES[gl_InstanceIndex]; \
    ARD_INSTANCE_IDX = gl_InstanceIndex; \
    ARD_MATERIAL_IDX = ARD_OBJECT_DATA[idx].material; \
    ARD_TEXTURES_IDX = ARD_OBJECT_DATA[idx].textures; \
    VsOut vs_out = func(); \
    ARD_FRAG_POS = vs_out.frag_pos; \
    ARD_FRAG_POS_VIEW_SPACE = vec3(camera.view * vec4(vs_out.frag_pos, 1.0)); \
} \

#else

#define ARD_ENTRY(func) \
void main() { \
    func(); \
} \

#endif

/////////////////
/// FUNCTIONS ///
/////////////////

/// Gets the model matrix for object.
mat4 get_model_matrix() {
    return ARD_OBJECT_DATA[ARD_OBJECT_INDICES[ARD_INSTANCE_IDX]].model;
}

#ifdef ARD_TEXTURE_COUNT
/// Samples a texture at a given slot. If the texture is unbound, the provided default will
/// be returned.
vec4 sample_texture_default(uint slot, vec2 uv, vec4 def) {
    uint tex = ARD_MATERIAL_TEXTURES[ARD_TEXTURES_IDX][slot];

    if (tex == NO_TEXTURE) {
        return def;
    } else {
        return texture(ARD_TEXTURES[tex], uv);
    }
}

/// Samples a texture at the given slot. Will return `vec4(0)` if the texture is unbound.
vec4 sample_texture(uint slot, vec2 uv) {
    return sample_texture_default(slot, uv, vec4(0));
}
#endif

#ifdef ARD_MATERIAL
/// Gets the material data for the object.
ARD_MATERIAL get_material_data() {
    return ARD_MATERIALS[ARD_MATERIAL_IDX];
}
#endif

#endif