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

void main() {
    uint rng_state = hit_value.rng_state;
    const vec3 sun_dir = hit_value.sun_dir.xyz;
    const uint object_id = gl_InstanceCustomIndexEXT;
    const uint meshlet_idx = gl_GeometryIndexEXT;
    const uint mesh_id = uint(object_data[object_id].mesh);
    const uint textures_slot = uint(object_data[object_id].textures);
    const mat3 normal_mat = mat3(object_data[object_id].model_inv);
    const uint meshlet_offset = mesh_info[mesh_id].meshlet_offset;
    const uint index_offset = v_meshlets[meshlet_offset + meshlet_idx].data.y;
    #if ARD_VS_HAS_UV0
        const uint color_tex = uint(texture_slots[textures_slot][0]);
        const uint normal_tex = uint(texture_slots[textures_slot][1]);
        const uint mr_tex = uint(texture_slots[textures_slot][2]);
    #endif
    const PbrMaterial mat_data = object_data[object_id].material.mat;
    const uint index_base = gl_PrimitiveID * 3;

    VertexAttribs verts = get_vertex_attribs(
        normal_mat,
        index_offset + index_base,
        attribs
    );

    // Sample textures
    #if ARD_VS_HAS_UV0
        vec4 color = sample_texture_default(color_tex, verts.uv0, vec4(1.0)) * mat_data.color;
        vec3 N = sample_texture_default(normal_tex, verts.uv0, vec4(0.5, 0.5, 1.0, 0.0)).xyz;
        const vec4 mr_map = sample_texture_default(mr_tex, verts.uv0, vec4(1.0));
    #else
        vec4 color = mat_data.color;
        vec3 N = vec3(0.5, 0.5, 1.0);
        const vec4 mr_map = vec4(1.0);
    #endif

    const vec3 V = -normalize(gl_WorldRayDirectionEXT);

    const mat3 TBN = mat3(
        verts.tangent,
        verts.bitangent,
        verts.normal
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