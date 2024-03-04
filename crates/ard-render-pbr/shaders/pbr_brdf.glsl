#ifndef _ARD_PBR_BRDF
#define _ARD_PBR_BRDF

const float INV_PI = 0.318309886184;
const float PI = 3.14159265359;

float specular_d(vec3 N, vec3 H, float roughness) {
    // GGX/Trowbridge-Reitz
    const float a = roughness * roughness;
    const float a2 = a * a;
    const float ndoth = dot(N, H);
    const float ndoth2 = ndoth * ndoth;

    const float nom = a2;
    float denom = (ndoth2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return nom / denom;
}

float geometry_schlick_ggx(float ndotv, float k) {
    const float denom = ndotv * (1.0 - k) + k;
    return ndotv / denom;
}

float specular_g(float ndotv, float ndotl, vec3 N, float roughness) {
    // Modified Schlick model
    float k = roughness + 1.0;
    k = (k * k) * 0.125;
    return geometry_schlick_ggx(ndotv, k) * geometry_schlick_ggx(ndotl, k);
}

vec3 specular_f(vec3 V, vec3 H, vec3 F0) {
    // Spherical gaussian approximation for Schlick Fresnel
    const float vdoth = dot(V, H);
    return F0 + (1.0 - F0) * pow(2.0, ((-5.55473 * vdoth) - 6.98316) * vdoth);
}

vec3 evaluate_brdf(
    vec3 albedo,
    vec3 F0,
    vec3 V,
    vec3 N,
    vec3 L,
    float metallic,
    float roughness,
    float ndotl
) {
    // Diffuse portion of the BRDF
    const vec3 f_diffuse = INV_PI * albedo;

    // Specular portion of the BRDF
    const vec3 H = normalize(L + V);
    const float ndotv = max(dot(N, V), 0.0);
    const float d = specular_d(N, H, roughness);
    const vec3 f = specular_f(V, H, F0);
    const float g = specular_g(ndotv, ndotl, N, roughness);

    const vec3 num = d * g * f;
    const float denom = (4.0 * ndotl * ndotv) + 0.0001;
    const vec3 f_specular = num / denom;

    // Ratio of reflected light is equal to the Fresnel parameter
    const vec3 kS = f;

    // Ratio of refracted light is the inverse of the reflected (energy conserving). We blend out
    // the diffuse portion for metals since dielectrics don't refract light.
    const vec3 kD = (1.0 - metallic) * (vec3(1.0) - kS);

    return (kD * f_diffuse) + (kS * f_specular);
}

#endif