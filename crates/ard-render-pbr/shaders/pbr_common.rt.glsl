#ifndef _ARD_PBR_COMMON_RT
#define _ARD_PBR_COMMON_RT

#include "pbr_brdf.glsl"

struct VertexAttribs {
    vec3 normal;
    vec3 tangent;
    vec3 bitangent;
#if ARD_VS_HAS_UV0
    vec2 uv0;
#endif
};

// Random number generation using pcg32i_random_t, using inc = 1. Our random state is a uint.
uint step_rng(uint rng_state) {
    return rng_state * 747796405 + 1;
}

// Steps the RNG and returns a floating-point value between 0 and 1 inclusive.
float rng_float(inout uint rng_state) {
    // Condensed version of pcg_output_rxs_m_xs_32_32, with simple conversion to floating-point [0,1].
    rng_state = step_rng(rng_state);
    uint word = ((rng_state >> ((rng_state >> 28) + 4)) ^ rng_state) * 277803737;
    word = (word >> 22) ^ word;
    return float(word) / 4294967295.0;
}

bool is_lambertian(
    float ndotv, 
    float metallic, 
    float roughness,
    vec3 color,
    vec3 F0,
    inout uint rng_state
) {
    const float dielectric_pr = 1.0 - roughness;
    float diff_pr = (1.0 - dielectric_pr);

    const float inv_total_pr = 1.0 / (diff_pr + dielectric_pr);
    diff_pr *= inv_total_pr;

    return rng_float(rng_state) <= roughness;
}

vec4 compute_brdf_pdf(
    vec3 L,
    vec3 N,
    vec3 V,
    vec3 color,
    vec3 F0,
    float roughness,
    float metallic
) {
    const float ndotl = max(dot(N, L), 0.0);

    // End early if the outputs would be too small
    if (ndotl < 0.0001) {
        return vec4(vec3(0.0), 1.0);
    }

    // Evaluate both lambertian and GGX brdfs
    const vec3 diffuse = color * INV_PI;
    const float diffuse_pdf = ndotl;

    const vec3 H = normalize(L + V);
    const float ndotv = max(dot(N, V), 0.0);
    const float vdoth = max(dot(V, H), 0.0);
    const float ndoth = max(dot(N, H), 0.0);

    const float d = specular_d(N, H, roughness);
    const vec3 f = specular_f(V, H, F0);
    const float g = specular_g(ndotv, ndotl, roughness);
    const vec3 num = d * g * f;
    const float denom = (4.0 * ndotl * ndotv) + 0.00001;
    vec3 spec = num / denom;
    const float spec_pdf = get_ggx_pdf(N, H, V, roughness);

    // Final PDF is the average
    const float pdf = (diffuse_pdf + spec_pdf) * 0.5;

    // Energy conserving blend of both PDFs
    const vec3 kS = f;
    const vec3 kD = (1.0 - metallic) * (vec3(1.0) - kS);

    return vec4(
        ndotl * ((kD * diffuse) + spec),
        pdf
    );
}

#ifdef ARD_SET_MESH_DATA
VertexAttribs get_vertex_attribs(
    const mat3 normal_mat,
    const uint index_base,
    vec2 attribs
) {
    vec3 normals[3];
    #if ARD_VS_HAS_TANGENT
    vec3 tangents[3];
    vec3 bitangents[3];
    #endif
    #if ARD_VS_HAS_UV0
    vec2 uvs[3];
    #endif

    [[unroll]]
    for (uint i = 0; i < 3; ++i) {
        const uint index = v_indices[index_base + i];

        const uvec2 ard_normal_raw = v_normals[index];
        vec4 ard_normal = vec4(
            unpackSnorm2x16(ard_normal_raw.x),
            unpackSnorm2x16(ard_normal_raw.y)
        );

        normals[i] = normalize(normal_mat * ard_normal.xyz);

        #if ARD_VS_HAS_UV0
            uvs[i] = unpackHalf2x16(v_uv0s[index]);
        #endif

        #if ARD_VS_HAS_TANGENT
            const uvec2 ard_tangent_raw = v_tangents[index];
            vec4 ard_tangent = vec4(
                unpackSnorm2x16(ard_tangent_raw.x),
                unpackSnorm2x16(ard_tangent_raw.y)
            );

            vec3 tangentW = normalize(normal_mat * ard_tangent.xyz);
            tangentW = normalize(tangentW - dot(tangentW, normals[i]) * normals[i]);
            vec3 bitangentW = normalize(cross(normals[i], tangentW) * ard_tangent.w);

            tangents[i] = tangentW;
            bitangents[i] = bitangentW;
        #endif
    }
    
    const vec3 bc = vec3(1.0 - attribs.x - attribs.y, attribs.x, attribs.y);

    VertexAttribs out_attribs;
    out_attribs.normal = normalize(normals[0] * bc.x + normals[1] * bc.y + normals[2] * bc.z);
    #if ARD_VS_HAS_UV0
    out_attribs.uv0 = uvs[0] * bc.x + uvs[1] * bc.y + uvs[2] * bc.z;
    #endif
    #if ARD_VS_HAS_TANGENT
    out_attribs.tangent = normalize(tangents[0] * bc.x + tangents[1] * bc.y + tangents[2] * bc.z);
    out_attribs.bitangent = 
        normalize(bitangents[0] * bc.x + bitangents[1] * bc.y + bitangents[2] * bc.z);
    #else
    vec3 tangent = abs(out_attribs.normal.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
    out_attribs.tangent = normalize(cross(out_attribs.normal, tangent));
    out_attribs.bitangent = normalize(cross(out_attribs.normal, out_attribs.tangent));
    #endif

    return out_attribs;
}
#endif

#endif