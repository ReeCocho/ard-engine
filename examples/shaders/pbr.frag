#version 450

struct PbrMaterial {
    vec4 base_color;
    float metallic;
    float roughness;
};

#define ARD_FRAGMENT_SHADER
#define ARD_MATERIAL PbrMaterial
#include "user_shaders.glsl"

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 SCREEN_POS;
layout(location = 1) in vec3 NORMAL;
layout(location = 2) in vec2 UV0;

const vec3 SLICE_TO_COLOR[FROXEL_TABLE_Z] = vec3[FROXEL_TABLE_Z](
    vec3(0.1),
    vec3(1.0),
    vec3(0.5),
    vec3(1.0, 0.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 0.0, 1.0),
    vec3(1.0, 1.0, 0.0),
    vec3(1.0, 0.0, 1.0),
    vec3(0.0, 1.0, 1.0),
    vec3(0.5, 0.0, 0.0),
    vec3(0.0, 0.5, 0.0),
    vec3(0.0, 0.0, 0.5),
    vec3(0.5, 0.5, 0.0),
    vec3(0.5, 0.0, 0.5),
    vec3(0.0, 0.5, 0.5),
    vec3(0.9)
);

void entry() {
    // Sample our albedo texture
    vec3 albedo = sample_texture_default(0, UV0, vec4(1)).xyz;

    // Get our material
    PbrMaterial material = get_material_data();

    // Compute lighting
    albedo = lighting(
        albedo.xyz * material.base_color.xyz,
        material.roughness,
        material.metallic,
        NORMAL,
        SCREEN_POS
    );

    FRAGMENT_COLOR = vec4(albedo, 1.0);
}

ARD_ENTRY(entry)