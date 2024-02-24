#version 450 core
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_multiview : enable

#define FRAGMENT_SHADER

#define ArdMaterialData PbrMaterial

#define ARD_SET_GLOBAL 0
#define ARD_SET_CAMERA 1
#define ARD_SET_TEXTURES 2
#define ARD_SET_MATERIALS 3

#define ARD_TEXTURE_COUNT 1

#include "ard_bindings.glsl"
#include "pbr_common.glsl"

#ifndef DEPTH_ONLY
layout(location = 0) out vec4 OUT_COLOR;
#endif

////////////////////
/// MAIN PROGRAM ///
////////////////////

void main() {
    const PbrMaterial data = ard_MaterialData(vs_Slots.w);

// Get color from diffuse texture
#if ARD_VS_HAS_UV0
    const vec4 color = sample_texture_default(vs_Slots.x, vs_Uv, vec4(1)) * data.color;
// Or just use the material color if we have no UVs
#else
    const vec4 color = data.color;
#endif

    // Alpha-Cutoff
    if (color.a < data.alpha_cutoff) {
        discard;
    }

// We only need to compute final color if we're not depth-only
#ifndef DEPTH_ONLY

    // Prefetch textures
    #if ARD_VS_HAS_UV0
        const vec4 mr_map = sample_texture_default(vs_Slots.y, vs_Uv, vec4(0.0, 1.0, 0.0, 0.0));
    #endif
    #if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0
        vec3 N = sample_texture_default(vs_Slots.z, vs_Uv, vec4(0.5, 0.5, 1.0, 0.0)).xyz;
    #endif

    // Apply material properties from texture
    #if ARD_VS_HAS_UV0
        const float metallic = clamp(data.metallic * mr_map.b, 0.0, 1.0);
        const float roughness = clamp(data.roughness * mr_map.g, 0.0, 1.0);
    #else
        const float metallic = clamp(data.metallic, 0.0, 1.0);
        const float roughness = clamp(data.roughness, 0.0, 1.0);
    #endif

    // If we have tangents and uvs, we can support normal mapping
    #if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0
        N = N * 2.0 - 1.0;
        N = normalize(vs_TBN * N);
    // Otherwise, we just use the vertex shader supplied normal
    #else
        vec3 N = normalize(vs_Normal);
    #endif

    // View vector
    const vec3 V = normalize(camera[gl_ViewIndex].position.xyz - vs_WorldSpaceFragPos);

    // Calculate reflectance at normal incidence; if dia-electric (like plastic) use F0 
    // of 0.04 and if it's a metal, use the albedo color as F0 (metallic workflow)    
    vec3 F0 = vec3(0.04); 
    F0 = mix(F0, color.rgb, metallic);

    // Default color is black
    vec4 final_color = vec4(0.0, 0.0, 0.0, color.a);

    // Lighting from the sun
    const vec3 frag_to_sun = -normalize(global_lighting.sun_direction.xyz);
    final_color += vec4(light_fragment(
        global_lighting.sun_color_intensity.rgb * global_lighting.sun_color_intensity.a,
        compute_shadow_factor(N),
        color.rgb,
        roughness,
        metallic,
        F0,
        frag_to_sun,
        V,
        N
    ), 0.0);

    // Lighting from point lights
    const vec2 screen_uv = (vs_Position.xy / vs_Position.w) * vec2(0.5) + vec2(0.5);
    const float screen_depth = (vs_Position.w * camera[gl_ViewIndex].near_clip) / vs_Position.z;
    const uvec3 cluster = get_cluster_id(screen_uv, screen_depth);

    int light_index = 0;
    uint light_idx = light_table.clusters[cluster.z][cluster.x][cluster.y][light_index];
    while (light_idx != FINAL_LIGHT_SENTINEL) {
        const Light light = lights[light_idx];

        vec3 frag_to_light = light.position_range.xyz - vs_WorldSpaceFragPos;
        const float dist_to_light = length(frag_to_light);
        frag_to_light /= dist_to_light;

        if (dist_to_light < light.position_range.w) {
            final_color += vec4(light_fragment(
                light.color_intensity.rgb,
                light_attenuation(dist_to_light, light.position_range.w) * light.color_intensity.w,
                color.rgb,
                roughness,
                metallic,
                F0,
                frag_to_light,
                V,
                N
            ), 0.0);
        }

        light_index += 1;
        light_idx = light_table.clusters[cluster.z][cluster.x][cluster.y][light_index];
    }

    const vec3 ambient_color = texture(di_map, N).rgb;
    const vec3 kS = fresnel_schlick_roughness(max(dot(N, V), 0.0), F0, roughness);
    const vec3 kD = (1.0 - kS) * (1.0 - metallic);
    const float ao = texture(ao_image, vec2(screen_uv.x, 1.0 - screen_uv.y)).r;
    const vec3 ambient = ao
        // * global_lighting.ambient_color_intensity.a
        * color.rgb
        * ambient_color;
        // * global_lighting.ambient_color_intensity.rgb;
    final_color += vec4(ambient, 0.0);

    OUT_COLOR = final_color;
#endif
}