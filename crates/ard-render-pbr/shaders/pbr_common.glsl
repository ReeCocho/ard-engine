#ifndef _ARD_PBR_COMMON
#define _ARD_PBR_COMMON

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

#endif