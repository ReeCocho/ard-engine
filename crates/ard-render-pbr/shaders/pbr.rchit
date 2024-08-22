#version 460
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_control_flow_attributes : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_buffer_reference : require

#define PATH_TRACE_PASS
#include "pbr_common.glsl"
#include "pbr_common.rt.glsl"

layout(location = 0) rayPayloadInEXT PathTracerPayload hit_value;
hitAttributeEXT vec2 attribs;

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

void main() {
    uint rng_state = hit_value.rng_state;
    const vec3 sun_dir = hit_value.sun_dir.xyz;
    const uint object_id = gl_InstanceCustomIndexEXT;
    const uint meshlet_idx = gl_GeometryIndexEXT;
    const uint mesh_id = uint(object_data[object_id].mesh);
    const uint textures_slot = uint(object_data[object_id].textures);
    #if !ARD_VS_HAS_TANGENT
    const mat4x3 model_mat = transpose(object_data[object_id].model);
    #endif
    const mat3 normal_mat = mat3(object_data[object_id].model_inv);
    const uint meshlet_offset = mesh_info[mesh_id].meshlet_offset;
    const uint index_offset = v_meshlets[meshlet_offset + meshlet_idx].data.y;
    const uint color_tex = uint(texture_slots[textures_slot][0]);
    const uint normal_tex = uint(texture_slots[textures_slot][1]);
    const uint mr_tex = uint(texture_slots[textures_slot][2]);
    const PbrMaterial mat_data = object_data[object_id].material.mat;
    const uint index_base = gl_PrimitiveID * 3;

    // Lookup UVs
    vec2 uvs[3];
    vec3 normals[3];
    #if ARD_VS_HAS_TANGENT
    vec3 tangents[3];
    vec3 bitangents[3];
    #else
    vec3 positions[3];
    #endif
    [[unroll]]
    for (uint i = 0; i < 3; i++) {
        uint index = v_indices[index_offset + index_base + i];

        const uvec2 ard_normal_raw = v_normals[index];
        vec4 ard_normal = vec4(
            unpackSnorm2x16(ard_normal_raw.x),
            unpackSnorm2x16(ard_normal_raw.y)
        );

        normals[i] = normalize(normal_mat * ard_normal.xyz);
        uvs[i] = unpackHalf2x16(v_uv0s[index]);

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
        #else
            positions[i] = model_mat * v_positions[index];
        #endif
    }

    // Compute barycentrics
    const vec3 bc = vec3(1.0 - attribs.x - attribs.y, attribs.x, attribs.y);

    // Compute final UV from barycentrics
    const vec2 uv = uvs[0] * bc.x + uvs[1] * bc.y + uvs[2] * bc.z;

    // Sample textures
    vec4 color = sample_texture_default(color_tex, uv, vec4(1.0)) * mat_data.color;
    vec3 N = sample_texture_default(normal_tex, uv, vec4(0.5, 0.5, 1.0, 0.0)).xyz;
    const vec4 mr_map = sample_texture_default(mr_tex, uv, vec4(1.0));

    const vec3 normal = normalize(normals[0] * bc.x + normals[1] * bc.y + normals[2] * bc.z);
    const vec3 V = -normalize(gl_WorldRayDirectionEXT);

    // Compute an ortho-normal basis from the surface normal
    #if ARD_VS_HAS_TANGENT
        // TBN for normal mapping
        const vec3 tangent = normalize(tangents[0] * bc.x + tangents[1] * bc.y + tangents[2] * bc.z);
        const vec3 bitangent = normalize(bitangents[0] * bc.x + bitangents[1] * bc.y + bitangents[2] * bc.z);
    #else
        vec3 tangent = abs(normal.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
        tangent = normalize(cross(normal, tangent));
        const vec3 bitangent = normalize(cross(normal, tangent));
    #endif

    const mat3 TBN = mat3(
        tangent,
        bitangent,
        normal
    );

    N = (N * 2.0) - vec3(1.0);
    N = normalize(TBN * N);

    // Surface properties
    const float metallic = clamp(mat_data.metallic * mr_map.b, 0.0, 1.0);
    const float roughness = clamp(mat_data.roughness * mr_map.g, 0.0, 1.0);

    const float ndotv = max(dot(N, V), 0.0);
    vec3 F0 = vec3(0.04);

    // Decide if we're lambertian (alternative is specular/metallic)
    const bool lambertian = is_lambertian(
        ndotv,
        metallic,
        roughness,
        color.rgb,
        vec3(0.04),
        rng_state
    );

    // Upate F0 based on metallicness
    F0 = mix(F0, color.rgb, metallic);

    // Compute BRDF/PDF for the sun direction
    hit_value.in_brdf_pdf = compute_brdf_pdf(sun_dir, N, V, color.rgb, F0, roughness, metallic);
    
    // Generate a new ray direction and position and apply BRDF
    vec3 L = vec3(0.0);
    vec3 H = vec3(0.0);
    float pdf = 0.0;
    const vec2 rand_vec = vec2(rng_float(rng_state), rng_float(rng_state));

    // Compute L and H based on our BRDF
    if (lambertian) {
        L = normalize(TBN * get_cosine_hemisphere(rand_vec));
        H = normalize(L + V);
    } else {
        H = normalize(TBN * get_ggx_microfacet(rand_vec, roughness));
        L = normalize(reflect(-V, H));
    }

    // Compute BRDF/PDF for the random direction
    hit_value.out_brdf_pdf = compute_brdf_pdf(L, N, V, color.rgb, F0, roughness, metallic);

    hit_value.sun_dir = vec4(L, 0.0);
    hit_value.rng_state = rng_state;
    hit_value.hit = 1;
    hit_value.location = vec4(
        (gl_WorldRayOriginEXT + gl_WorldRayDirectionEXT * gl_HitTEXT) + (N * 0.02), 1.0
    );
}