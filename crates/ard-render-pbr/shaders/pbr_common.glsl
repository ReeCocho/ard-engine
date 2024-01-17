#ifndef _ARD_PBR_COMMON
#define _ARD_PBR_COMMON

#ifdef VERTEX_SHADER
    #include "pbr_common.vs.glsl"
#endif

#ifdef FRAGMENT_SHADER
    #include "pbr_common.fs.glsl"
#endif

/////////////////
/// CONSTANTS ///
/////////////////

const float PI = 3.14159265359;

const int SHADOW_SAMPLE_COUNT = 16;

const vec2 SHADOW_SAMPLE_POINTS[64] = vec2[64](
    vec2(0.2146, -0.0948),
    vec2(0.0990, 0.3579),
    vec2(-0.0035, 0.2658),
    vec2(0.1762, -0.5591),
    vec2(-0.0004, 0.0009),
    vec2(0.6321, -0.6937),
    vec2(0.9553, 0.1622),
    vec2(-0.6298, 0.2715),
    vec2(0.1512, -0.0202),
    vec2(0.0000, 0.0000),
    vec2(-0.2233, 0.4659),
    vec2(-0.6553, 0.5411),
    vec2(0.0163, -0.7383),
    vec2(0.0017, -0.0059),
    vec2(-0.0180, 0.0081),
    vec2(-0.0599, -0.0180),
    vec2(-0.0022, 0.0003),
    vec2(0.0084, -0.0025),
    vec2(0.3367, 0.1045),
    vec2(0.0339, -0.2171),
    vec2(0.0001, -0.0295),
    vec2(-0.0688, -0.1093),
    vec2(0.2506, -0.3755),
    vec2(0.0090, 0.0079),
    vec2(0.4726, 0.0084),
    vec2(-0.1536, -0.1363),
    vec2(-0.0405, -0.3138),
    vec2(-0.1842, 0.0519),
    vec2(0.2597, 0.4207),
    vec2(0.1068, 0.1259),
    vec2(-0.2395, 0.1493),
    vec2(-0.4052, -0.0651),
    vec2(0.0979, 0.8154),
    vec2(-0.2605, 0.2911),
    vec2(-0.8371, -0.2679),
    vec2(-0.0023, 0.0155),
    vec2(0.0297, -0.0189),
    vec2(0.0374, 0.0173),
    vec2(-0.1076, 0.6513),
    vec2(-0.0205, -0.0133),
    vec2(-0.2703, -0.1281),
    vec2(-0.0590, 0.0901),
    vec2(0.6532, -0.3993),
    vec2(-0.2345, -0.8777),
    vec2(0.0065, 0.0474),
    vec2(0.0399, 0.0892),
    vec2(-0.0417, 0.0357),
    vec2(0.0001, 0.0002),
    vec2(0.7138, 0.3460),
    vec2(0.0597, -0.1273),
    vec2(0.0871, 0.0132),
    vec2(0.5890, -0.1601),
    vec2(-0.0520, 0.1702),
    vec2(0.2086, 0.1379),
    vec2(-0.0026, -0.0030),
    vec2(-0.3601, -0.4321),
    vec2(0.4713, 0.4256),
    vec2(-0.5904, -0.3978),
    vec2(0.0522, -0.0594),
    vec2(-0.0194, -0.0678),
    vec2(-0.1182, -0.0010),
    vec2(-0.5352, 0.0666),
    vec2(0.2558, -0.2151),
    vec2(-0.1723, -0.3947)
);

// Intensity of lighting attenuation we consider to be "close enough" to 0.
const float ATTENUATION_EPSILON = 0.001;

/// Inverse square attenuation that works based off of range.
///
/// `x` - Distance from the light source.
/// `range` - Range of the light source.
float light_attenuation(float x, float range) {
    // Variation of inverse square falloff that allows for attenuation of 0 at x = range
    const float s = x / range;
    
    if (s >= 1.0) return 0.0;

    float s2 = s * s;
    s2 = 1.0 - s2;
    s2 = s2 * s2;

    return s2 / (1.0 + s);
}

/// Trowbridge-Reitz GGZ normal distribution function for approximating surface area of 
/// microfacets.
///
/// `N` - Surface normal.
/// `H` - The halfway vector.
/// `roughness` - Linear roughness.
///
/// Values closer to 0 are more specular. Values closer to 1 are more rough.
float distribution_ggx(vec3 N, vec3 H, float roughness) {
    const float a = roughness * roughness;
    const float a2 = a * a;
    const float NdotH = max(dot(N, H), 0.0);
    const float NdotH2 = NdotH * NdotH;

    const float nom = a2;
    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return nom / denom;
}

float geometry_schlick_ggx(float NdotV, float roughness) {
    const float r = (roughness + 1.0);
    const float k = (r * r) / 8.0;

    const float nom = NdotV;
    const float denom = NdotV * (1.0 - k) + k;

    return nom / denom;
}

/// Schlick approximation of Smith using GGX for approximating geometry shadowing factor.
///
/// `NdotV` - Surface normal dotted with the viewing vector.
/// `roughness` - Linear roughness.
///
/// Values closer to 0 represent higher microfacet shadowing. Values closer to 1 represent lower
/// microfacet shadowing.
float geometry_smith(vec3 N, vec3 V, vec3 L, float roughness) {
    const float NdotV = max(dot(N, V), 0.0);
    const float NdotL = max(dot(N, L), 0.0);
    const float ggx2 = geometry_schlick_ggx(NdotV, roughness);
    const float ggx1 = geometry_schlick_ggx(NdotL, roughness);
    return ggx1 * ggx2;
}

/// Fresnel Schlick approximation of light refractance.
///
/// `NdotH` - Surface normal dotted with the halfway vector.
/// `F0` - Base reflectivity of the surface (index of refraction).
///
/// Returns ratio of light reflected over the ratio that gets refracted.
vec3 fresnel_schlick(float NdotH, vec3 F0) {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - NdotH, 0.0, 1.0), 5.0);
}

/// Fresnel Schlick approximation of light refractance taking into account surface roughness.
///
/// `NdotH` - Surface normal dotted with the halfway vector.
/// `F0` - Base reflectivity of the surface (index of refraction).
/// `roughness` - Linear roughness.
///
/// Returns ratio of light reflected over the ratio that gets refracted.
vec3 fresnel_schlick_roughness(float cos_theta, vec3 F0, float roughness) {
    return F0 + (max(vec3(1.0 - roughness), F0) - F0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

/// Samples the shadow cascade at a given UV.
///
/// `cascade` - Index of the shadow cascade to sample.
/// `uv` - UV coordinate within the cascade to sample.
/// `bias` - Sampling bias.
/// `filter_radius_uv` - UV radius to perform PCF within.
/// `z_receiver` - Z coordinate in light space for the shadow receiver.
float sample_shadow_map(int layer, vec2 uv, float bias, vec2 filter_radius_uv, float z_receiver) {
    float shadow = 0.0;
    vec3 jcoord = vec3((vs_WorldSpaceFragPos.xz + vec2(vs_WorldSpaceFragPos.yy)) * 100.0, 0.0);
    vec2 sm_coord = uv;
    vec4 fr_uv2 = vec4(filter_radius_uv, filter_radius_uv);

    for (int i = 0; i < SHADOW_SAMPLE_COUNT; i++) {
        vec2 offset = filter_radius_uv * SHADOW_SAMPLE_POINTS[i] * 4.0;
        shadow += texture(shadow_cascades[layer], vec3(uv + offset, z_receiver - bias)).r / float(SHADOW_SAMPLE_COUNT);
    }

    return shadow;
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

    vec4 frag_pos_light_space = vs_LightSpacePositions[layer]; 

    float NoL = dot(normal, global_lighting.sun_direction.xyz);
    float bias = max(
        sun_shadow_info.cascades[layer].max_bias * (1.0 - NoL), 
        sun_shadow_info.cascades[layer].min_bias
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

    // Cook-Torrance BRDF
    const vec3 H = normalize(V + L);
    const float NDF = distribution_ggx(N, H, roughness);
    const float G = geometry_smith(N, V, L, roughness);
    const vec3 F = fresnel_schlick(max(dot(H, V), 0.0), F0);

    // Bias to avoid divide by 0
    const vec3 numerator = NDF * G * F;
    const float denominator = 4.0 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.0001;
    const vec3 specular = numerator / denominator;

    // kS is Fresnel
    const vec3 kS = F;

    // For energy conservation, the diffuse and specular light can't be above 1.0 (unless
    // the surface emits light); to preserve this relationship the diffuse component (kD)
    // should be equal to 1.0 - kS
    //
    // Multiply kD by the inverse metalness such that only non-metals have diffuse
    // lighting, or a linear blend if partly metal (pure metals have no diffuse light).
    const vec3 kD = (vec3(1.0) - kS) * (1.0 - metallic);

    // Scale light by NdotL
    const float NdotL = max(dot(N, L), 0.0);

    // Add to outgoing radiance Lo
    // NOTE: We already multiplied the BRDF by the Fresnel (kS) so we won't multiply
    // by kS again
    return (kD * base_color / PI + specular) * radiance * NdotL;
}

/// Get the cluster ID for the given screen coordinate.
ivec3 get_cluster_id(vec2 uv, float depth) {
    return ivec3(
        clamp(int(uv.x * float(CAMERA_FROXELS_WIDTH)), 0, CAMERA_FROXELS_WIDTH - 1),
        clamp(int(uv.y * float(CAMERA_FROXELS_HEIGHT)), 0, CAMERA_FROXELS_HEIGHT - 1),
        clamp(
            int(log(depth) * camera.cluster_scale_bias.x - camera.cluster_scale_bias.y), 
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

/// Samples a texture at a given slot. If the texture is unbound, the provided default will be 
/// returned.
vec4 sample_texture_default(uint slot, vec2 uv, vec4 def) {
    const uint tex = texture_slots[vs_TextureSlotsIdx][slot];
    if (tex == EMPTY_TEXTURE_ID) {
        return def;
    } else {
        return texture(textures[tex], uv);
    }
}

/// Samples a texture at a given slot. Will return `vec4(0)` if the texture is unbound.
vec4 sample_texture(uint slot, vec2 uv) {
    return sample_texture_default(slot, uv, vec4(0));
}

#endif

/// Bindless material data.
#ifdef ArdMaterialData
ArdMaterialData get_material_data() {
    return material_data[vs_MaterialDataSlotIdx];
}
#endif

#endif