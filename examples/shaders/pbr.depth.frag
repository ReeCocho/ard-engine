#version 450 core

struct PbrMaterial {
    vec4 base_color;
    float metallic;
    float roughness;
    float alpha_cutoff;
};

#define ARD_FRAGMENT_SHADER
#define ARD_TEXTURE_COUNT 3
#define ARD_MATERIAL PbrMaterial
#include "ard_std.glsl"

layout(location = 0) in vec4 SCREEN_POS;
layout(location = 1) in vec4 NORMAL;
layout(location = 2) in vec2 UV;

void entry() {
    PbrMaterial material = get_material_data();
    vec4 tex_color = sample_texture_default(0, UV, vec4(1));

    if (tex_color.a < material.alpha_cutoff) {
        discard;
    }
}
ARD_ENTRY(entry)
