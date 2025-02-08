#version 450 core
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_multiview : enable

#define FRAGMENT_SHADER
#include "pbr_common.glsl"
#include "utils.glsl"

#if defined(COLOR_PASS) || defined(TRANSPARENT_PREPASS)
    layout(location = 0) out vec4 OUT_COLOR;
#if !defined(TRANSPARENT_COLOR_PASS)
    layout(location = 1) out vec4 OUT_KS_RGH;
    layout(location = 2) out vec4 OUT_VEL;
    layout(location = 3) out vec4 OUT_NORM;
    layout(location = 4) out vec4 OUT_TAN;
#endif
#endif

#ifdef ENTITY_PASS
    layout(location = 0) out uint OUT_ENTITY_ID;
#endif

#if defined(DEPTH_PREPASS)
    layout(location = 0) out vec4 ALPHA_TO_COVERAGE_OUT;
#endif

float calc_mip_level(const vec2 texture_coord)
{
    const vec2 dx = dFdx(texture_coord);
    const vec2 dy = dFdy(texture_coord);
    const float delta_max_sqr = max(dot(dx, dx), dot(dy, dy));
    return max(0.0, 0.5 * log2(delta_max_sqr));
}

////////////////////
/// MAIN PROGRAM ///
////////////////////

void main() {
    const PbrMaterial data = object_data[vs_in.slots.w].material.mat;

// Get color from diffuse texture
#if ARD_VS_HAS_UV0
    vec4 color = sample_texture_default(vs_in.slots.x, vs_in.uv, vec4(1)) * data.color;
    const vec2 tex_size = vs_in.slots.x >= MAX_TEXTURES
        ? vec2(0.0)
        : vec2(textureSize(textures[vs_in.slots.x], 0));
// Or just use the material color if we have no UVs
#else
    vec4 color = data.color;
    const vec2 tex_size = vec2(0.0);
#endif

#if !defined(TRANSPARENT_COLOR_PASS) && !defined(SHADOW_PASS)
    // Alpha-to-coverage fix for MSAA when alpha testing geometry.
    // https://bgolus.medium.com/anti-aliased-alpha-test-the-esoteric-alpha-to-coverage-8b177335ae4f

    // This is to scale the cuttoff by mip level to prevent fading out at far distances
    const float mip_scale = 0.25;
    #if ARD_VS_HAS_UV0
        const float mip_level = calc_mip_level(vs_in.uv * tex_size);
    #else
        const float mip_level = 0.0;
    #endif
    color.a *= 1.0 + max(0.0, mip_level) * mip_scale;

    // This is to sharpen the alpha to the width of a single texel
    color.a = (color.a - data.alpha_cutoff) / max(fwidth(color.a), 0.0001) + 0.5;
#endif
    
#if defined(ALPHA_CUTOFF_PASS)
    // Alpha-Cutoff
    if (color.a < data.alpha_cutoff) {
        discard;
    }
#endif

#if defined(DEPTH_PREPASS)
    ALPHA_TO_COVERAGE_OUT.a = color.a;
#endif

// Entity pass just needs to output the entity ID
#if defined(ENTITY_PASS)
    OUT_ENTITY_ID = vs_in.entity;
    return;
#endif

// We need to enter here in both color and depth prepass. Depth prepass needs the thin G buffer.
#if defined(COLOR_PASS) || defined(DEPTH_PREPASS)

    // Prefetch textures
    #if ARD_VS_HAS_UV0
        const vec4 mr_map = sample_texture_default(vs_in.slots.y, vs_in.uv, vec4(1.0));
    #endif
    #if ARD_VS_HAS_TANGENT && ARD_VS_HAS_UV0
        vec3 N = sample_texture_default(vs_in.slots.z, vs_in.uv, vec4(0.5, 0.5, 1.0, 0.0)).xyz;
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
        const mat3 tbn = mat3(
            normalize(vs_in.tangent),
            normalize(vs_in.bitangent),
            normalize(vs_in.normal)
        );
        N = (N * 2.0) - vec3(1.0);
        N = normalize(tbn * N);
        const vec3 T = tbn[0];
    // Otherwise, we just use the vertex shader supplied normal
    #else
        vec3 N = normalize(vs_in.normal);
        const vec3 T = normalize(
            cross(N, abs(N.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0))
        );
    #endif

    // View vector
    const vec3 V = normalize(camera[gl_ViewIndex].position.xyz - vs_in.world_space_position);

    // Calculate reflectance at normal incidence; if dielectric (like plastic) use F0 
    // of 0.04 and if it's a metal, use the albedo color as F0 (metallic workflow)    
    vec3 F0 = vec3(0.04); 
    F0 = mix(F0, color.rgb, metallic);

    // Everything past here is for color passes only
#if !defined(DEPTH_PREPASS)
    // For ambient lighting
    const vec3 kS = specular_f_roughness(max(dot(N, V), 0.0), F0, roughness);
    const vec3 kD = (1.0 - metallic) * (vec3(1.0) - kS);

    const vec2 ndc_position = vs_in.ndc_position.xy / vs_in.ndc_position.w;

#if !defined(TRANSPARENT_COLOR_PASS)
    vec2 last_pos = vs_in.ndc_last_position.xy / vs_in.ndc_last_position.w;
    last_pos.y = -last_pos.y;
    last_pos = (last_pos + vec2(1.0)) * vec2(0.5);

    vec2 cur_pos = ndc_position;
    cur_pos.y = -cur_pos.y;
    cur_pos = (cur_pos + vec2(1.0)) * vec2(0.5);

    OUT_KS_RGH = vec4(kS, roughness);
    OUT_VEL = vec4(cur_pos - last_pos, 0.0, 0.0);
    OUT_NORM = vec4(N.xyz, 0.0);
    OUT_TAN = vec4(T.xyz, 0.0);
#endif

#if !defined(TRANSPARENT_PREPASS)
    const vec2 screen_uv = ndc_position * vec2(0.5) + vec2(0.5);

    // Default color is ambient
    const vec3 diffuse = color.rgb
        * texture(di_map, N).rgb 
        * global_lighting.ambient_color_intensity.rgb;

    const vec3 ambient = global_lighting.ambient_color_intensity.a * kD * diffuse;

    vec4 final_color = vec4(
        texture(ao_image, vec2(screen_uv.x, 1.0 - screen_uv.y)).r * vec3(ambient),
        color.a
    );

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
    const float screen_depth = (vs_in.ndc_position.w * camera[gl_ViewIndex].near_clip) 
        / vs_in.ndc_position.z;
    const uvec3 cluster = get_cluster_id(screen_uv, screen_depth);

    int light_index = 0;
    uint light_idx = light_table.clusters[cluster.z][cluster.x][cluster.y][light_index];
    while (light_idx != FINAL_LIGHT_SENTINEL) {
        const Light light = lights[light_idx];

        vec3 frag_to_light = light.position_range.xyz - vs_in.world_space_position;
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

    /*
    // Reflection vector modified based on roughness
    vec3 R = reflect(-V, N);
    const float fa = roughness * roughness;
    R = mix(N, R, (1.0 - fa) * (sqrt(1.0 - fa) + fa));
    const float REFLECTION_LODS = float(textureQueryLevels(env_map) - 1);
    const vec3 prefiltered_color = 
        textureLod(env_map, R, roughness * REFLECTION_LODS).rgb 
        * global_lighting.ambient_color_intensity.rgb;
    const vec2 env_brdf = texture(brdf_lut, vec2(max(dot(N, V), 0.0), roughness)).rg;
    const vec3 specular = prefiltered_color * (kS * env_brdf.x + env_brdf.y);
    */

    // NOTE: Old `ambient` that included the specular term 
    // global_lighting.ambient_color_intensity.a * (kD * diffuse + specular);
    
    // Ambient occlusion term used here

    OUT_COLOR = final_color;
#endif
#endif
#endif
}