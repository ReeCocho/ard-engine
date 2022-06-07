#ifndef _USER_SHADERS_GLSL
#define _USER_SHADERS_GLSL

#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

/// Common descriptors and helper functions to use for vertex shaders.

#include "data_structures.glsl"

const float PI = 3.14159265359;

#define SHADOW_SAMPLES_COUNT 32
#define SHADOW_SAMPLES_COUNT_DIV2 16
#define SHADOW_SAMPLES_COUNT_INV (1.0 / SHADOW_SAMPLES_COUNT)

#ifdef ARD_VERTEX_SHADER
layout(location = 16) flat out uint ARD_INSTANCE_IDX;
layout(location = 17) out vec3 ARD_FRAG_POS;
layout(location = 18) out vec3 ARD_FRAG_POS_VIEW_SPACE;
layout(location = 19) out vec4 ARD_FRAG_POS_LIGHT_SPACE[MAX_SHADOW_CASCADES];
layout(location = 19 + MAX_SHADOW_CASCADES) out vec3 ARD_FRAG_POS_LIGHT_VIEW_SPACE[MAX_SHADOW_CASCADES];
#endif

#ifdef ARD_FRAGMENT_SHADER
layout(location = 16) flat in uint ARD_INSTANCE_IDX;
layout(location = 17) in vec3 ARD_FRAG_POS;
layout(location = 18) in vec3 ARD_FRAG_POS_VIEW_SPACE;
layout(location = 19) in vec4 ARD_FRAG_POS_LIGHT_SPACE[MAX_SHADOW_CASCADES];
layout(location = 19 + MAX_SHADOW_CASCADES) in vec3 ARD_FRAG_POS_LIGHT_VIEW_SPACE[MAX_SHADOW_CASCADES];
#endif

//////////////
/// GLOBAL ///
//////////////

layout(set = 0, binding = 0) readonly buffer ARD_InputInfoData {
    ObjectInfo[] ARD_OBJECT_INFO;
};

layout(set = 0, binding = 1) readonly buffer ARD_PointLights {
    PointLight[] ARD_POINT_LIGHTS;
};

layout(set = 0, binding = 2) readonly buffer ARD_InputObjectIndices {
    uint[] ARD_OBJECT_INDICES;
};

layout(set = 0, binding = 3) readonly buffer ARD_PointLightTable {
    PointLightTable ARD_POINT_LIGHT_TABLE;
};

layout(set = 0, binding = 4) uniform ARD_Lighting {
    Lighting ARD_LIGHTING;
};

layout(set = 0, binding = 5) uniform sampler2D[MAX_SHADOW_CASCADES] ARD_SHADOW_MAPS;

layout(set = 0, binding = 6) uniform sampler3D ARD_POISSON_DISK;

layout(set = 0, binding = 7) uniform samplerCube ARD_SKYBOX;

layout(set = 0, binding = 8) uniform samplerCube ARD_IRRADIANCE_MAP;

layout(set = 0, binding = 9) uniform samplerCube ARD_RADIANCE_MAP;

layout(set = 0, binding = 10) uniform sampler2D IBL_BRDF_LUT;

////////////////
/// TEXTURES ///
////////////////

layout(set = 1, binding = 0) uniform sampler2D[] ARD_TEXTURES;

//////////////
/// CAMERA ///
//////////////

layout(set = 2, binding = 0) uniform ARD_Camera {
    Camera camera;
};

layout(set = 2, binding = 1) readonly buffer ARD_CameraClusterFroxels {
    CameraClusterFroxels ARD_CAMERA_CLUSTER_FROXELS;
};

/////////////////
/// MATERIALS ///
/////////////////

layout(set = 3, binding = 0) readonly buffer ARD_TextureData {
    uint[][MAX_TEXTURES_PER_MATERIAL] ARD_MATERIAL_TEXTURES;
};

#ifdef ARD_MATERIAL
layout(set = 3, binding = 1) readonly buffer ARD_MaterialData {
    ARD_MATERIAL[] ARD_MATERIALS;
};
#endif

/////////////////
/// FUNCTIONS ///
/////////////////

#ifdef ARD_VERTEX_SHADER

#define ARD_ENTRY(func) \
void main() { \
    ARD_INSTANCE_IDX = gl_InstanceIndex; \
    VsOut vs_out = func(); \
    ARD_FRAG_POS = vs_out.frag_pos; \
    ARD_FRAG_POS_VIEW_SPACE = vec3(camera.view * vec4(vs_out.frag_pos, 1.0)); \
    for (int i = 0; i < MAX_SHADOW_CASCADES; ++i) { \
        ARD_FRAG_POS_LIGHT_VIEW_SPACE[i] = \
            vec3(ARD_LIGHTING.cascades[i].view * vec4(vs_out.frag_pos, 1.0)); \
        ARD_FRAG_POS_LIGHT_SPACE[i] = ARD_LIGHTING.cascades[i].vp * vec4(vs_out.frag_pos, 1.0); \
    } \
} \

#else

#define ARD_ENTRY(func) \
void main() { \
    func(); \
} \

#endif

/// Gets the model matrix for object.
mat4 get_model_matrix() {
    #ifdef ARD_VERTEX_SHADER
    uint idx = gl_InstanceIndex;
    #endif

    #ifdef ARD_FRAGMENT_SHADER
    uint idx = ARD_INSTANCE_IDX;
    #endif

    return ARD_OBJECT_INFO[ARD_OBJECT_INDICES[idx]].model;
}

/// Samples a texture at a given slot. If the texture is unbound, the provided default will
/// be returned.
vec4 sample_texture_default(uint slot, vec2 uv, vec4 def) {
    #ifdef ARD_VERTEX_SHADER
    uint idx = gl_InstanceIndex;
    #endif

    #ifdef ARD_FRAGMENT_SHADER
    uint idx = ARD_INSTANCE_IDX;
    #endif

    uint tex = ARD_MATERIAL_TEXTURES[ARD_OBJECT_INFO[ARD_OBJECT_INDICES[idx]].textures][slot];

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

#ifdef ARD_MATERIAL
/// Gets the material data for the object.
ARD_MATERIAL get_material_data() {
    #ifdef ARD_VERTEX_SHADER
    uint idx = gl_InstanceIndex;
    #endif

    #ifdef ARD_FRAGMENT_SHADER
    uint idx = ARD_INSTANCE_IDX;
    #endif

    return ARD_MATERIALS[ARD_OBJECT_INFO[ARD_OBJECT_INDICES[idx]].material];
}
#endif

float distribution_GGX(vec3 N, vec3 H, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float NdotH = max(dot(N, H), 0.0);
    float NdotH2 = NdotH * NdotH;

    float nom = a2;
    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return nom / denom;
}

float geometry_schlick_GGX(float NdotV, float roughness) {
    float r = (roughness + 1.0);
    float k = (r * r) / 8.0;

    float nom = NdotV;
    float denom = NdotV * (1.0 - k) + k;

    return nom / denom;
}

float geometry_smith(vec3 N, vec3 V, vec3 L, float roughness) {
    float NdotV = max(dot(N, V), 0.0);
    float NdotL = max(dot(N, L), 0.0);
    float ggx2 = geometry_schlick_GGX(NdotV, roughness);
    float ggx1 = geometry_schlick_GGX(NdotL, roughness);

    return ggx1 * ggx2;
}

vec3 fresnel_schlick(float cos_theta, vec3 F0) {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

vec3 fresnel_schlick_roughness(float cos_theta, vec3 F0, float roughness) {
    return F0 + (max(vec3(1.0 - roughness), F0) - F0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}  

#extension GL_EXT_debug_printf : enable

// PCSS code here:
// https://developer.download.nvidia.com/whitepapers/2008/PCSS_Integration.pdf

float pcf_filter(vec2 uv, float z_receiver, float bias, vec2 filter_radius_uv, int layer) {
    float shadow = 0.0;
    vec3 jcoord = vec3(ARD_FRAG_POS.xy, 0.0);
    vec2 sm_coord = uv;
    vec4 fr_uv2 = vec4(filter_radius_uv, filter_radius_uv);

    // // Perform cheap tests first
    // for (int i = 0; i < 4; i++) {
    //     vec4 offset = texture(ARD_POISSON_DISK, jcoord) * fr_uv2;
    //     jcoord.z += 1.0 / SHADOW_SAMPLES_COUNT;
    //     
    //     shadow += z_receiver - bias < texture(ARD_SHADOW_MAPS[layer], uv + offset.xy).r ? (1.0 / 8) : 0.0;
    //     shadow += z_receiver - bias < texture(ARD_SHADOW_MAPS[layer], uv + offset.zw).r ? (1.0 / 8) : 0.0;
    // }

    // If all test samples are either 0's or 1's, we can skip expensive filtering
    //if ((shadow - 1) * shadow != 0) {
        //shadow *= 8.0 / SHADOW_SAMPLES_COUNT;
        
        for (int i = 0; i < SHADOW_SAMPLES_COUNT; ++i) {
            vec4 offset = texture(ARD_POISSON_DISK, jcoord) * fr_uv2;
            jcoord.z += 1.0 / SHADOW_SAMPLES_COUNT;

            shadow += z_receiver - bias < texture(ARD_SHADOW_MAPS[layer], uv + offset.xy).r ? 
                SHADOW_SAMPLES_COUNT_INV * 0.5 : 0.0;

            shadow += z_receiver - bias < texture(ARD_SHADOW_MAPS[layer], uv + offset.zw).r ? 
                SHADOW_SAMPLES_COUNT_INV * 0.5 : 0.0;
        }
    //}
    
    return shadow;
}

float shadow_calculation(vec3 normal) {
    // Determine which cascade to use
    int layer = MAX_SHADOW_CASCADES - 1;
    for (int i = 0; i < MAX_SHADOW_CASCADES; ++i) {
        if (ARD_FRAG_POS_VIEW_SPACE.z < ARD_LIGHTING.cascades[i].far_plane) {
            layer = i;
            break;
        }
    }

    vec4 frag_pos_light_space = ARD_FRAG_POS_LIGHT_SPACE[layer]; 

    float NoL = dot(normal, ARD_LIGHTING.sun_direction.xyz);
    float bias = max(
        ARD_LIGHTING.cascades[layer].max_bias * (1.0 - NoL), 
        ARD_LIGHTING.cascades[layer].min_bias
    ) * (1.0 / ARD_LIGHTING.cascades[layer].depth_range);

    vec3 proj_coords = frag_pos_light_space.xyz / frag_pos_light_space.w;
    proj_coords.xy = proj_coords.xy * 0.5 + 0.5;
    proj_coords.y = 1.0 - proj_coords.y;

    vec2 filter_radius_uv = 0.05 * ARD_LIGHTING.cascades[layer].uv_size;

	// Filtering
	return pcf_filter(
        proj_coords.xy, 
        proj_coords.z, 
        bias, 
        filter_radius_uv, 
        layer
    );
}

/// light_color - As named.
/// base_color - Albedo factory of material.
/// roughness - Roughness factor.
/// metallic - Metallic factor.
/// attenuation - As named.
/// L - Direction from fragment to light.
/// V - Direction from fragment to camera.
/// N - Surface normal.
/// F0 - Reflectance at normal incidence.
vec3 lighting_general(
    vec3 light_color,
    vec3 base_color,
    float roughness,
    float metallic,
    float attenuation,
    vec3 L,
    vec3 V,
    vec3 N,
    vec3 F0
) {
    // Per light radiance
    vec3 H = normalize(V + L);
    vec3 radiance = light_color * attenuation;

    // Cook-Torrance BRDF
    float NDF = distribution_GGX(N, H, roughness);
    float G = geometry_smith(N, V, L, roughness);
    vec3 F = fresnel_schlick(max(dot(H, V), 0.0), F0);

    // Bias to avoid divide by 0
    vec3 numerator = NDF * G * F;
    float denominator = 4.0 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.0001;
    vec3 specular = numerator / denominator;

    // kS is Fresnel
    vec3 kS = F;

    // For energy conservation, the diffuse and specular light can't be above 1.0 (unless
    // the surface emits light); to preserve this relationship the diffuse component (kD)
    // should be equal to 1.0 - kS
    vec3 kD = vec3(1.0) - kS;

    // Multiply kD by the inverse metalness such that only non-metals have diffuse
    // lighting, or a linear blend if partly metal (pure metals have no diffuse light).
    kD *= 1.0 - metallic;

    // Scale light by NdotL
    float NdotL = max(dot(N, L), 0.0);

    // Add to outgoing radiance Lo
    // NOTE: We already multiplied the BRDF by the Fresnel (kS) so we won't multiply
    // by kS again
    return (kD * base_color / PI + specular) * radiance * NdotL;
}

vec3 irradiance_ambient(
    vec3 base_color,
    float roughness,
    float metallic,
    vec3 V,
    vec3 N,
    vec3 R,
    vec3 F0
) {
    vec3 F = fresnel_schlick_roughness(max(dot(N, V), 0.0), F0, roughness);

    vec3 kS = F;
    vec3 kD = 1.0 - kS;
    kD *= 1.0 - metallic;

    vec3 irradiance = texture(ARD_IRRADIANCE_MAP, N).rgb;
    vec3 diffuse = irradiance * base_color;

    vec3 radiance = textureLod(ARD_RADIANCE_MAP, R, roughness * float(ARD_LIGHTING.radiance_mip_count)).rgb;
    vec2 env_brdf = texture(IBL_BRDF_LUT, vec2(max(dot(N, V), 0.0), roughness)).rg;
    vec3 specular = radiance * (F * env_brdf.x + env_brdf.y);

    return (kD * diffuse) + specular;
}

/// Performs PBR lighting calculations.
///
/// base_color : Albedo.
/// roughness : Roughness factor.
/// metallic : Metallic factor.
/// normal : Surface normal.
/// screen_pos : The fragments screen position BEFORE perspective divide.
vec3 lighting(
    vec3 base_color, 
    float roughness, 
    float metallic, 
    vec3 normal, 
    vec4 screen_pos
) {
    // Compute the fragment position 
    vec3 world_pos = ARD_FRAG_POS;

    // Determine which cluster the fragment is in
    vec2 uv = ((screen_pos.xy / screen_pos.w) * 0.5) + vec2(0.5);

    ivec3 cluster = ivec3(
        clamp(int(uv.x * float(FROXEL_TABLE_X)), 0, FROXEL_TABLE_X - 1),
        clamp(int(uv.y * float(FROXEL_TABLE_Y)), 0, FROXEL_TABLE_Y - 1),
        clamp(
            int(log(screen_pos.z) * camera.scale_bias.x - camera.scale_bias.y), 
            0, 
            FROXEL_TABLE_Z - 1
        )
    );

    // Determine the number of point lights
    int count = ARD_POINT_LIGHT_TABLE.light_counts[cluster.z][cluster.x][cluster.y];

    // Normal in world space
    vec3 N = normalize(normal);

    // Vector from fragment to the camera
    vec3 V = normalize(camera.position.xyz - world_pos);

    // Reflectance vector
    vec3 R = reflect(-V, N);

    // Calculate reflectance at normal incidence; if dia-electric (like plastic) use F0 
    // of 0.04 and if it's a metal, use the albedo color as F0 (metallic workflow)    
    vec3 F0 = vec3(0.04); 
    F0 = mix(F0, base_color, metallic);
    
    vec3 Lo = vec3(0.0);

    // Directional light
    float shadow = shadow_calculation(N);
    float sun_attenuation = shadow;

    Lo += lighting_general(
        ARD_LIGHTING.sun_color_intensity.xyz * ARD_LIGHTING.sun_color_intensity.w,
        base_color,
        roughness,
        metallic,
        sun_attenuation,
        -normalize(ARD_LIGHTING.sun_direction.xyz),
        V,
        N,
        F0
    );

    // Point lights
    for (int i = 0; i < count; i++) {
        uint light_idx = ARD_POINT_LIGHT_TABLE.clusters[cluster.z][cluster.x][cluster.y][i];
        PointLight light = ARD_POINT_LIGHTS[light_idx];

        vec3 frag_to_light = light.position_range.xyz - world_pos;
        float dist_to_light = length(frag_to_light);

        if (dist_to_light < light.position_range.w) {
            vec3 L = normalize(frag_to_light);
            float sqr_dist = dist_to_light * dist_to_light;
            float sqr_range = light.position_range.w * light.position_range.w;
            float attenuation = (1.0 - (sqr_dist / sqr_range)) * light.color_intensity.w;
            
            Lo += lighting_general(
                light.color_intensity.xyz,
                base_color,
                roughness,
                metallic,
                attenuation,
                L,
                V,
                N,
                F0
            );
        }
    }

    vec3 ambient = 
        ARD_LIGHTING.ambient.xyz * 
        ARD_LIGHTING.ambient.w * 
        irradiance_ambient(
            base_color,
            roughness,
            metallic,
            V,
            N,
            R,
            F0
        );

    return ambient + Lo;
}

#endif