#version 450 core
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_multiview : enable

#define ARD_TEXTURE_COUNT 3
#define FRAGMENT_SHADER
#define ArdMaterialData PbrMaterial
#include "pbr_common.glsl"
#include "utils.glsl"

#ifdef COLOR_PASS
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
    
    // Alpha-Cutoff
    #if defined(ALPHA_CUTOFF_PASS)
    if (color.a < data.alpha_cutoff) {
        discard;
    }
    #endif
// Or just use the material color if we have no UVs
#else
    const vec4 color = data.color;
#endif

// We only need to compute final color if we're not depth-only
#ifdef COLOR_PASS

    // Prefetch textures
    #if ARD_VS_HAS_UV0
        const vec4 mr_map = sample_texture_default(vs_Slots.y, vs_Uv, vec4(1.0));
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

    // Reflection vector modified based on roughness
    vec3 R = reflect(-V, N);
    const float fa = roughness * roughness;
    R = mix(N, R, (1.0 - fa) * (sqrt(1.0 - fa) + fa));

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

    // Ambient lighting
    const vec3 kS = specular_f_roughness(max(dot(N, V), 0.0), F0, roughness);
    const vec3 kD = (1.0 - metallic) * (vec3(1.0) - kS);

    const vec3 diffuse = color.rgb
        * texture(di_map, N).rgb 
        * global_lighting.ambient_color_intensity.rgb;

    const float REFLECTION_LODS = float(textureQueryLevels(env_map) - 1);
    const vec3 prefiltered_color = 
        textureLod(env_map, R, roughness * REFLECTION_LODS).rgb 
        * global_lighting.ambient_color_intensity.rgb;
    const vec2 env_brdf = texture(brdf_lut, vec2(max(dot(N, V), 0.0), roughness)).rg;
    const vec3 specular = prefiltered_color * (kS * env_brdf.x + env_brdf.y);

    const vec3 ambient = global_lighting.ambient_color_intensity.a * (kD * diffuse + specular);
    final_color += vec4(ambient, 0.0);

    // Ambient occlusion
    final_color.rgb = texture(ao_image, vec2(screen_uv.x, 1.0 - screen_uv.y)).r * final_color.rgb;
    // final_color = vec4(vec3(ambient_attenuation), 1.0);

    OUT_COLOR = final_color;
#endif
}