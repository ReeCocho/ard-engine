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

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 SCREEN_POS;
layout(location = 1) in vec4 NORMAL;
layout(location = 2) in vec2 UV;
layout(location = 3) in mat3 TBN;

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

float pcf_filter(vec2 uv, float z_receiver, float bias, vec2 filter_radius_uv, int layer) {
    float shadow = 0.0;
    vec3 jcoord = vec3((ARD_FRAG_POS.xz + vec2(ARD_FRAG_POS.yy)) * 100.0, 0.0);
    vec2 sm_coord = uv;
    vec4 fr_uv2 = vec4(filter_radius_uv, filter_radius_uv);

    for (int x = -2; x <= 2; ++x) {
        for (int y = -2; y <= 2; ++y) {
            vec2 offset = filter_radius_uv * vec2(x, y);
            shadow += texture(ARD_SHADOW_MAPS[layer], vec3(uv + offset, z_receiver - bias)).r / 25.0;
        }
    }
    
    return shadow;
}

float shadow_calculation(vec3 normal) {
    if (ARD_SHADOW_INFO.cascade_count == 0) {
        return 1.0;
    }
    
    // Determine which cascade to use
    int layer = int(ARD_SHADOW_INFO.cascade_count) - 1;
    for (int i = 0; i < ARD_SHADOW_INFO.cascade_count; ++i) {
        if (ARD_FRAG_POS_VIEW_SPACE.z < ARD_SHADOW_INFO.cascades[i].far_plane) {
            layer = i;
            break;
        }
    }

    vec4 frag_pos_light_space = ARD_FRAG_POS_LIGHT_SPACE[layer]; 

    float NoL = dot(normal, ARD_LIGHTING_INFO.sun_direction.xyz);
    float bias = max(
        ARD_SHADOW_INFO.cascades[layer].max_bias * (1.0 - NoL), 
        ARD_SHADOW_INFO.cascades[layer].min_bias
    ) * (1.0 / ARD_SHADOW_INFO.cascades[layer].depth_range);

    vec3 proj_coords = frag_pos_light_space.xyz / frag_pos_light_space.w;
    proj_coords.xy = proj_coords.xy * 0.5 + 0.5;
    proj_coords.y = 1.0 - proj_coords.y;

    vec2 filter_radius_uv = 0.01 * ARD_SHADOW_INFO.cascades[layer].uv_size;

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
    // Determine which cluster the fragment is in
    vec3 world_pos = ARD_FRAG_POS;
    vec2 uv = ((screen_pos.xy / screen_pos.w) * 0.5) + vec2(0.5);
    ivec3 cluster = ivec3(
        clamp(int(uv.x * float(FROXEL_TABLE_X)), 0, FROXEL_TABLE_X - 1),
        clamp(int(uv.y * float(FROXEL_TABLE_Y)), 0, FROXEL_TABLE_Y - 1),
        clamp(
            int(log(screen_pos.z) * camera.cluster_scale_bias.x - camera.cluster_scale_bias.y), 
            0, 
            FROXEL_TABLE_Z - 1
        )
    );

    // Determine the number of point lights
    uint count = ARD_CLUSTERS.light_counts[cluster.z][cluster.x][cluster.y];

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
    float sun_attenuation = shadow_calculation(N);

    Lo += lighting_general(
        ARD_LIGHTING_INFO.sun_color_intensity.xyz * ARD_LIGHTING_INFO.sun_color_intensity.w,
        base_color,
        roughness,
        metallic,
        sun_attenuation,
        -normalize(ARD_LIGHTING_INFO.sun_direction.xyz),
        V,
        N,
        F0
    );

    // Point lights
    for (int i = 0; i < count; i++) {
        uint light_idx = ARD_CLUSTERS.clusters[cluster.z][cluster.x][cluster.y][i];
        Light light = ARD_LIGHTS[light_idx];

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
        ARD_LIGHTING_INFO.ambient_color_intensity.xyz * 
        ARD_LIGHTING_INFO.ambient_color_intensity.w * 
        base_color;

    return ambient + Lo;
}

void entry() {
    /*
    vec3 world_pos = ARD_FRAG_POS;
    vec2 uv = ((SCREEN_POS.xy / SCREEN_POS.w) * 0.5) + vec2(0.5);
    ivec3 cluster = ivec3(
        clamp(int(uv.x * float(FROXEL_TABLE_X)), 0, FROXEL_TABLE_X - 1),
        clamp(int(uv.y * float(FROXEL_TABLE_Y)), 0, FROXEL_TABLE_Y - 1),
        clamp(
            int(log(SCREEN_POS.z) * camera.cluster_scale_bias.x - camera.cluster_scale_bias.y), 
            0, 
            FROXEL_TABLE_Z - 1
        )
    );

    vec3 color = vec3(0.1);
    uint count = ARD_CLUSTERS.light_counts[cluster.z][cluster.x][cluster.y];
    for (int i = 0; i < count; i++) {
        uint light_idx = ARD_CLUSTERS.clusters[cluster.z][cluster.x][cluster.y][i];
        debugPrintfEXT("%u %u", count, light_idx);
        Light light = ARD_LIGHTS[light_idx];
        float dist = length(light.position_range.xyz - world_pos);
        float sqr_dist = dist * dist;
        float sqr_range = light.position_range.w * light.position_range.w;
        float attenuation = (1.0 - (sqr_dist / sqr_range)) * light.color_intensity.w;
        color += vec3(clamp(attenuation, 0.0, 1.0));
    }
    */
    PbrMaterial material = get_material_data();
    vec4 tex_color = sample_texture_default(0, UV, vec4(1));

    if (tex_color.a < material.alpha_cutoff) {
        discard;
    }

    vec4 met_rgh = sample_texture_default(2, UV, vec4(0.0, 1.0, 0.0, 0.0));
    vec3 normal = sample_texture_default(1, UV, vec4(0.5, 0.5, 1.0, 0.0)).xyz;
    normal = normal * 2.0 - 1.0;
    normal = normalize(TBN * normal);

    vec3 color = lighting(
        tex_color.rgb * material.base_color.rgb,
        material.roughness * met_rgh.g,
        material.metallic * met_rgh.b,
        normal,
        SCREEN_POS
    );

    FRAGMENT_COLOR = vec4(color, 1.0);
}
ARD_ENTRY(entry)
