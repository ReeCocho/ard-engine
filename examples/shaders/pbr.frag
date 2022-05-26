#version 450
#extension GL_ARB_separate_shader_objects : enable

#define TABLE_X 32
#define TABLE_Y 16
#define TABLE_Z 16
#define MAX_POINT_LIGHTS 256

const float PI = 3.14159265359;

struct PointLight {
    vec4 color_intensity;
    vec4 position_range;
};

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 FRAG_POS;
layout(location = 1) in vec3 WORLD_POS;
layout(location = 2) in vec3 NORMAL;
layout(location = 3) flat in uint INSTANCE_IDX;

struct ObjectInfo {
    mat4 model;
    uint material;
    uint textures;
};

struct PbrMaterial {
    vec4 base_color;
    float metallic;
    float roughness;
};

layout(set = 0, binding = 0) readonly buffer InputInfoData {
    ObjectInfo[] objects;
};

layout(set = 0, binding = 2) readonly buffer InputObjectIdxs {
    uint[] obj_idxs;
};

layout(set = 0, binding = 1) readonly buffer PointLights {
    PointLight[] lights;
};

layout(set = 0, binding = 3) readonly buffer PointLightTable {
    int[TABLE_Z][TABLE_X][TABLE_Y] light_counts;
    uint[TABLE_Z][TABLE_X][TABLE_Y][MAX_POINT_LIGHTS] clusters; 
};

layout(set = 2, binding = 0) uniform CameraUBO {
    mat4 view;
    mat4 projection;
    mat4 vp;
    mat4 view_inv;
    mat4 projection_inv;
    mat4 vp_inv;
    vec4[6] planes;
    vec4 properties;
    vec4 position;
    vec2 scale_bias;
} camera;

layout(set = 3, binding = 1) readonly buffer MaterialData {
    PbrMaterial[] materials;
};

const vec3 SLICE_TO_COLOR[TABLE_Z] = vec3[TABLE_Z](
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

float distribution_GGX(vec3 N, vec3 H, float roughness);
float geometry_schlick_GGX(float NdotV, float roughness);
float geometry_smith(vec3 N, vec3 V, vec3 L, float roughness);
vec3 fresnel_schlick(float cosTheta, vec3 F0);

void main() {
    // Determine which cluster the fragment is in
    vec2 uv = ((FRAG_POS.xy / FRAG_POS.w) * 0.5) + vec2(0.5);

    ivec3 cluster = ivec3(
        clamp(int(uv.x * float(TABLE_X)), 0, TABLE_X - 1),
        clamp(int(uv.y * float(TABLE_Y)), 0, TABLE_Y - 1),
        clamp(int(log(FRAG_POS.z) * camera.scale_bias.x - camera.scale_bias.y), 0, TABLE_Z - 1)
    );

    int count = light_counts[cluster.z][cluster.x][cluster.y];
    
    // Grab the material
    PbrMaterial material = materials[objects[obj_idxs[INSTANCE_IDX]].material];

    // Normal in world space
    vec3 N = NORMAL;

    // Vector from fragment to the camera
    vec3 V = normalize(camera.position.xyz - WORLD_POS);

    // Calculate reflectance at normal incidence; if dia-electric (like plastic) use F0 
    // of 0.04 and if it's a metal, use the albedo color as F0 (metallic workflow)    
    vec3 F0 = vec3(0.04); 
    F0 = mix(F0, material.base_color.xyz, material.metallic);
    
    vec3 Lo = vec3(0.0);

    for (int i = 0; i < count; i++) {
        PointLight light = lights[clusters[cluster.z][cluster.x][cluster.y][i]];
        vec3 frag_to_light = light.position_range.xyz - WORLD_POS;
        float dist_to_light = length(frag_to_light);
        if (dist_to_light < light.position_range.w) {
            // Per light radiance
            vec3 L = normalize(frag_to_light);
            vec3 H = normalize(V + L);
            float sqr_dist = dist_to_light * dist_to_light;
            float sqr_range = light.position_range.w * light.position_range.w;
            float attenuation = (1.0 - (sqr_dist / sqr_range)) * light.color_intensity.w;
            vec3 radiance = light.color_intensity.xyz * attenuation;

            // Cook-Torrance BRDF
            float NDF = distribution_GGX(N, H, material.roughness);
            float G = geometry_smith(N, V, L, material.roughness);
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
            kD *= 1.0 - material.metallic;

            // Scale light by NdotL
            float NdotL = max(dot(N, L), 0.0);

            // Add to outgoing radiance Lo
            // NOTE: We already multiplied the BRDF by the Fresnel (kS) so we won't multiply
            // by kS again
            Lo += (kD * material.base_color.xyz / PI + specular) * radiance * NdotL;
        }
    }

    vec3 ambient = vec3(0.03) * material.base_color.xyz;    
    vec3 color = ambient + Lo;

    FRAGMENT_COLOR = vec4(color, 1.0);
}

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