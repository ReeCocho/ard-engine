#ifndef _ARD_PBR_COMMON
#define _ARD_PBR_COMMON

#ifdef VERTEX_SHADER
    #include "pbr_common.vs.glsl"
#endif

#ifdef FRAGMENT_SHADER
    #include "pbr_common.fs.glsl"
#endif

#include "pbr_brdf.glsl"

/////////////////
/// CONSTANTS ///
/////////////////

const int SHADOW_KERNEL_SIZE = 2;
const int SHADOW_SAMPLE_COUNT = (1 + (2 * SHADOW_KERNEL_SIZE)) * (1 + (2 * SHADOW_KERNEL_SIZE));

// Intensity of lighting attenuation we consider to be "close enough" to 0.
const float ATTENUATION_EPSILON = 0.001;

/// Inverse square attenuation that works based off of range.
///
/// `x` - Distance from the light source.
/// `range` - Range of the light source.
float light_attenuation(float x, float range) {
    // Variation of inverse square falloff that allows for attenuation of 0 at x = range
    // and constant brightness regardless of range.
    return pow(clamp(1.0 - pow(x / range, 4.0), 0.0, 1.0), 2.0) / ((x * x) + 1.0);
}

#ifndef DEPTH_ONLY
/// Samples the shadow cascade at a given UV.
///
/// `cascade` - Index of the shadow cascade to sample.
/// `uv` - UV coordinate within the cascade to sample.
/// `bias` - Sampling bias.
/// `filter_radius_uv` - UV radius to perform PCF within.
/// `z_receiver` - Z coordinate in light space for the shadow receiver.
float sample_shadow_map(int layer, vec2 uv, float bias, vec2 filter_radius_uv, float z_receiver) {
    float shadow = 0.0;
    const vec3 jcoord = vec3((vs_WorldSpaceFragPos.xz + vec2(vs_WorldSpaceFragPos.yy)) * 100.0, 0.0);
    const vec2 sm_coord = uv;
    const vec4 fr_uv2 = vec4(filter_radius_uv, filter_radius_uv);
    const float shadow_bias = z_receiver - bias;

    for (int x = -SHADOW_KERNEL_SIZE; x <= SHADOW_KERNEL_SIZE; x++) {
        for (int y = -SHADOW_KERNEL_SIZE; y <= SHADOW_KERNEL_SIZE; y++) {
            vec2 offset = filter_radius_uv * (vec2(x, y) * vec2(1.0 / float(SHADOW_KERNEL_SIZE))) * 1.5;
            shadow += texture(shadow_cascades[layer], vec3(uv + offset, shadow_bias)).r;
        }
    }

    return shadow / float(SHADOW_SAMPLE_COUNT);
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
        if (vs_ViewSpacePosition.z < sun_shadow_info.cascades[i].far_plane) {
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
            vs_WorldSpaceFragPos + (sun_shadow_info.cascades[layer].normal_bias * normalize(vs_Normal)),
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

#ifdef VERTEX_SHADER
    #define ard_ObjectId (object_ids[gl_InstanceIndex])
#endif

#define ard_ModelMatrix(ID) (object_data[ID].model)
#define ard_NormalMatrix(ID) (object_data[ID].normal)
#define ard_TextureSlot(ID) (object_data[ID].textures)
#define ard_MaterialSlot(ID) (object_data[ID].material)

/// Bindless texture sampling.
#ifdef ARD_TEXTURE_COUNT

/// Samples a texture at a given texture ID. If the texture is unbound, the provided default will 
/// be returned.
vec4 sample_texture_default(uint id, vec2 uv, vec4 def) {
    return mix(
        texture(textures[min(id, MAX_TEXTURES - 1)], uv), 
        def, 
        float(id == EMPTY_TEXTURE_ID)
    );
}

/// Samples a texture at a given slot. Will return `vec4(0)` if the texture is unbound.
vec4 sample_texture(uint slot, vec2 uv) {
    return sample_texture_default(slot, uv, vec4(0));
}

#endif

/// Bindless material data.
#ifdef ArdMaterialData
    #define ard_MaterialData(ID) (material_data[ID])
#endif

#endif