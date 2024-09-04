#ifndef _ARD_PBR_COMMON
#define _ARD_PBR_COMMON

#extension GL_EXT_buffer_reference : require

///////////////
/// SET DEF ///
///////////////

#if defined(COLOR_PASS) 
    #define ARD_SET_COLOR_PASS 0
    #define COLOR_PASS
#endif

#if defined(TRANSPARENT_COLOR_PASS)
    #define ARD_SET_TRANSPARENT_PASS 0
    #define COLOR_PASS
#endif

#if defined(SHADOW_PASS)
    #define ARD_SET_SHADOW_PASS 0
#endif

#if defined(HIGH_Z_PASS)
    #define ARD_SET_HZB_PASS 0
#endif

#if defined(DEPTH_PREPASS)
    #define ARD_SET_DEPTH_PREPASS 0
#endif

#if defined(ENTITY_PASS)
    #define ARD_SET_ENTITY_PASS 0
#endif

#if defined(PATH_TRACE_PASS)
    #define ARD_SET_PATH_TRACER_PASS 0
#endif

#if defined(REFLECTIONS_PASS)
    #define ARD_SET_REFLECTIONS_PASS 0
#endif

#define ARD_SET_CAMERA 1
#define ARD_SET_MESH_DATA 2
#define ARD_SET_TEXTURE_SLOTS 3
#define ARD_SET_TEXTURES 4

layout(std430, buffer_reference, buffer_reference_align = 8) readonly buffer ArdMaterialPtr;
#define ArdMaterial ArdMaterialPtr
#include "ard_bindings.glsl"

layout(std430, buffer_reference, buffer_reference_align = 8) readonly buffer ArdMaterialPtr { PbrMaterial mat; };

///////////////
/// STRUCTS ///
///////////////

struct MsPayload {
    mat4x3 model;
#if defined(COLOR_PASS)
    mat4x3 last_model;
#endif
    mat3 normal;
    uint object_id;
    uint meshlet_base;
    uint meshlet_info_base;
#if defined(ENTITY_PASS)
    uint entity;
#endif
#if ARD_VS_HAS_UV0
    uint color_tex;
    uint met_rough_tex;
#if ARD_VS_HAS_TANGENT
    uint normal_tex;
#endif
#endif
};

////////////////
/// INCLUDES ///
////////////////

#include "pbr_brdf.glsl"

#ifdef MESH_SHADER
    #include "pbr_common.ms.glsl"
#endif

#ifdef TASK_SHADER
    #include "pbr_common.ts.glsl"
#endif

#ifdef FRAGMENT_SHADER
    #include "pbr_common.fs.glsl"
#endif

/////////////////
/// CONSTANTS ///
/////////////////

// Intensity of lighting attenuation we consider to be "close enough" to 0.
const float ATTENUATION_EPSILON = 0.001;

/////////////////
/// FUNCTIONS ///
/////////////////

/// Inverse square attenuation that works based off of range.
///
/// `x` - Distance from the light source.
/// `range` - Range of the light source.
float light_attenuation(float x, float range) {
    // Variation of inverse square falloff that allows for attenuation of 0 at x = range
    // and constant brightness regardless of range.
    return pow(clamp(1.0 - pow(x / range, 4.0), 0.0, 1.0), 2.0) / ((x * x) + 1.0);
}

#if defined(COLOR_PASS) && defined(FRAGMENT_SHADER)
/// Samples the shadow cascade at a given UV.
///
/// `cascade` - Index of the shadow cascade to sample.
/// `uv` - UV coordinate within the cascade to sample.
/// `bias` - Sampling bias.
/// `filter_radius_uv` - UV radius to perform PCF within.
/// `z_receiver` - Z coordinate in light space for the shadow receiver.
float sample_shadow_map(int layer, vec2 uv, float bias, vec2 filter_radius_uv, float z_receiver) {
    float shadow = 0.0;
    const vec3 jcoord = vec3(
        (vs_in.world_space_position.xz + vec2(vs_in.world_space_position.yy)) * 100.0, 0.0
    );
    const vec2 sm_coord = uv;
    const vec4 fr_uv2 = vec4(filter_radius_uv, filter_radius_uv);
    const float shadow_bias = z_receiver - bias;

    // Take half the shadow samples first
    for (uint i = 0; i < SUN_SHADOW_KERNEL_SIZE / 2; i++) {
        const vec2 kernel_off = unpackSnorm2x16(sun_shadow_info.kernel[i]);
        vec2 offset = filter_radius_uv * kernel_off;
        shadow += texture(shadow_cascades[layer], vec3(uv + offset, shadow_bias)).r;
    }

    // If we have a full dark shadow, early out
    if (shadow == 0.0) {
        return 0.0;
    }

    // Take the rest of the samples
    for (uint i = SUN_SHADOW_KERNEL_SIZE / 2; i < SUN_SHADOW_KERNEL_SIZE; i++) {
        const vec2 kernel_off = unpackSnorm2x16(sun_shadow_info.kernel[i]);
        vec2 offset = filter_radius_uv * kernel_off;
        shadow += texture(shadow_cascades[layer], vec3(uv + offset, shadow_bias)).r;
    }

    return shadow / float(SUN_SHADOW_KERNEL_SIZE);
}

/// Calculates the shadowing factor of the fragment with the given surface normal.
///
/// NOTE: Even though this is called the "shadow factor", really what it's getting is the
/// coefficient for the lighting for the fragment, so really it's the "inverse shadow factor."
///
/// `normal` - Surface normal.
float compute_shadow_factor(vec3 normal) {
    // Determine which cascade to use
    int layer = int(sun_shadow_info.count);
    for (int i = 0; i < sun_shadow_info.count; ++i) {
        if (vs_in.view_space_position.z < sun_shadow_info.cascades[i].far_plane) {
            layer = i;
            break;
        }
    }

    // Outside shadow bounds
    if (layer == sun_shadow_info.count) {
        return 1.0;
    }

    const vec4 frag_pos_light_space = sun_shadow_info.cascades[layer].vp 
        * vec4(
            vs_in.world_space_position 
            + (sun_shadow_info.cascades[layer].normal_bias * normalize(vs_in.normal)),
            1.0
        );

    float NoL = dot(normal, global_lighting.sun_direction.xyz);
    float bias = max(
        sun_shadow_info.cascades[layer].max_depth_bias * (1.0 - NoL), 
        sun_shadow_info.cascades[layer].min_depth_bias
    ) * (1.0 / sun_shadow_info.cascades[layer].depth_range);

    vec3 proj_coords = frag_pos_light_space.xyz / frag_pos_light_space.w;
    proj_coords.xy = proj_coords.xy * 0.5 + 0.5;
    proj_coords.y = 1.0 - proj_coords.y;

    vec2 filter_radius_uv = 0.01 * sun_shadow_info.cascades[layer].uv_size;

	// Filtering
	return sample_shadow_map(
        layer,
        proj_coords.xy, 
        bias, 
        filter_radius_uv, 
        proj_coords.z
    );
}
#endif

/// Computes lighting from a generic source.
///
/// `light_color` - Color of the light.
/// `attenuation` - Attenuation factor of the light.
/// `base_color` - Base color of the fragment being lit.
/// `roughness` - Roughness factor of the fragment being lit.
/// `metallic` - Metallic factor of the fragment being lit.
/// `F0` - Reflectance at normal incidence.
/// `L` - Direction from the fragment to the light.
/// `V` - Direction from the fragment to the camera.
/// `N` - Surface normal.
vec3 light_fragment(
    vec3 light_color,
    float attenuation,
    vec3 base_color,
    float roughness,
    float metallic,
    vec3 F0,
    vec3 L,
    vec3 V,
    vec3 N
) {
    // Per light radiance
    const vec3 radiance = light_color * attenuation;

    // Evaluate the BRDF
    const float ndotl = max(dot(N, L), 0.0);
    const vec3 brdf = evaluate_brdf(base_color, F0, V, N, L, metallic, roughness, ndotl);

    return brdf * radiance * ndotl;
}

/// Get the cluster ID for the given screen coordinate.
#if !defined(TASK_SHADER) && !defined(PATH_TRACE_PASS) && !defined(REFLECTIONS_PASS)
uvec3 get_cluster_id(vec2 uv, float depth) {
    return uvec3(
        clamp(uint(uv.x * float(CAMERA_FROXELS_WIDTH)), 0, CAMERA_FROXELS_WIDTH - 1),
        clamp(uint(uv.y * float(CAMERA_FROXELS_HEIGHT)), 0, CAMERA_FROXELS_HEIGHT - 1),
        clamp(
            uint(log(depth) * camera[gl_ViewIndex].cluster_scale_bias.x - camera[gl_ViewIndex].cluster_scale_bias.y), 
            0,
            CAMERA_FROXELS_DEPTH - 1
        )
    );
}
#endif

vec4 sample_texture_default_bias(uint id, vec2 uv, float bias, vec4 def) {
    if (id != EMPTY_TEXTURE_ID) {
        def = textureLod(textures[min(id, MAX_TEXTURES - 1)], uv, bias);
    }
    return def;
}

/// Samples a texture at a given texture ID. If the texture is unbound, the provided default will
/// be returned.
vec4 sample_texture_default(uint id, vec2 uv, vec4 def) {
    if (id != EMPTY_TEXTURE_ID) {
        def = texture(textures[min(id, MAX_TEXTURES - 1)], uv);
    }
    return def;
}

/// Samples a texture at a given slot. Will return `vec4(0)` if the texture is unbound.
vec4 sample_texture(uint slot, vec2 uv) {
    return sample_texture_default(slot, uv, vec4(0));
}

#endif