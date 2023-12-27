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

/// Trowbridge-Reitz GGZ normal distribution function for approximating surface area of 
/// microfacets.
///
/// `N` - Surface normal.
/// `H` - The halfway vector.
/// `roughness` - Linear roughness.
///
/// Values closer to 0 are more specular. Values closer to 1 are more rough.
float distribution_ggx(vec3 N, vec3 H, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float NdotH = max(dot(N, H), 0.0);
    float NdotH2 = NdotH * NdotH;

    float nom = a2;
    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return nom / denom;
}

float geometry_schlick_ggx(float NdotV, float roughness) {
    float r = (roughness + 1.0);
    float k = (r * r) / 8.0;

    float nom = NdotV;
    float denom = NdotV * (1.0 - k) + k;

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
    float NdotV = max(dot(N, V), 0.0);
    float NdotL = max(dot(N, L), 0.0);
    float ggx2 = geometry_schlick_ggx(NdotV, roughness);
    float ggx1 = geometry_schlick_ggx(NdotL, roughness);

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

/// Get the cluster ID for the given screen coordinate.
ivec3 get_cluster_id(vec4 vpos) {
    vec2 uv = (vpos.xy / vpos.w) * vec2(0.5) + vec2(0.5);
    float depth = (vpos.w * camera.near_clip) / vpos.z;
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
    const vec4 tex_val = texture(textures[tex], uv);
    return mix(tex_val, def, float(tex == EMPTY_TEXTURE_ID));
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