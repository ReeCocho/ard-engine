#ifndef _ARD_PBR_BRDF
#define _ARD_PBR_BRDF

const float INV_PI = 0.318309886184;
const float PI = 3.14159265359;

float schlick_weight(float u) {
    float m = clamp(1.0 - u, 0.0, 1.0);
    float m2 = m * m;
    return m2 * m2 * m;
}

float specular_d(vec3 N, vec3 H, float roughness) {
    // GGX/Trowbridge-Reitz
    const float a = roughness * roughness;
    const float a2 = a * a;
    const float ndoth = dot(N, H);
    const float ndoth2 = ndoth * ndoth;

    const float nom = a2;
    float denom = (ndoth2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return nom / (denom + 0.00001);
}

float geometry_schlick_ggx(float ndotv, float k) {
    const float denom = ndotv * (1.0 - k) + k;
    return ndotv / (denom + 0.00001);
}

float specular_g(float ndotv, float ndotl, float roughness) {
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

vec3 specular_f_roughness(float ndotv, vec3 F0, float roughness) {
    return F0 + (max(vec3(1.0 - roughness), F0) - F0) * pow(clamp(1.0 - ndotv, 0.0, 1.0), 5.0);
}   

// Computes a random GGX microfacet direction.
// Computed in tangent space. Result should be transformed with TBN matrix.
vec3 get_ggx_microfacet(vec2 rand_vec, float roughness) {
    const float a2 = roughness * roughness;
    const float cos_theta_h = sqrt(max(0.0, (1.0 - rand_vec.x) / ((a2 - 1.0) * rand_vec.x + 1.0)));
	const float sin_theta_h = sqrt(max(0.0, 1.0 - cos_theta_h * cos_theta_h));
	const float phi_h = rand_vec.y * PI * 2.0;

    return vec3(
        cos(phi_h) * sin_theta_h,
        sin(phi_h) * sin_theta_h,
        cos_theta_h
    );
}

// Randomly samples a cosine weighted hemisphere.
// Computed in tangent space. Result should be transformed with TBN matrix.
vec3 get_cosine_hemisphere(vec2 rand_vec) {
    const float pdf = sqrt(rand_vec.x);
    const float theta = acos(pdf);
    const float phi = 2.0 * PI * rand_vec.y;

    return vec3(
        cos(phi) * sin(theta),
        sin(phi) * sin(theta),
        pdf
    );
}

float get_ggx_pdf(vec3 N, vec3 H, vec3 V, float roughness) {
    return (specular_d(N, H, roughness) * max(dot(N, H), 0.0))
        / (4.0 * max(dot(H, V), 0.0001));
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
    const float g = specular_g(ndotv, ndotl, roughness);

    const vec3 num = d * g * f;
    const float denom = (4.0 * ndotl * ndotv) + 0.00001;
    const vec3 f_specular = num / denom;

    // Ratio of reflected light is equal to the Fresnel parameter
    const vec3 kS = f;

    // Ratio of refracted light is the inverse of the reflected (energy conserving). We blend out
    // the diffuse portion for metals since dielectrics don't refract light.
    const vec3 kD = (1.0 - metallic) * (vec3(1.0) - kS);

    return (kD * f_diffuse) + f_specular;
}

#endif