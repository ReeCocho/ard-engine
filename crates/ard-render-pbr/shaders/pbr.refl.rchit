#version 460
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_control_flow_attributes : enable
#extension GL_EXT_ray_tracing : enable
#extension GL_EXT_buffer_reference : require

#define REFLECTIONS_PASS
#include "pbr_common.glsl"
#include "pbr_common.rt.glsl"

layout(location = 0) rayPayloadInEXT RtReflectionsPayload hit_value;
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
        // TODO: Maybe make this configurable?
        const float tex_lod = gl_HitTEXT * 0.12;
        vec4 color = sample_texture_default_bias(
            color_tex, 
            verts.uv0, 
            tex_lod,
            vec4(1.0)
        ) * mat_data.color;
        vec3 N = sample_texture_default_bias(
            normal_tex, 
            verts.uv0, 
            tex_lod,
            vec4(0.5, 0.5, 1.0, 0.0)
        ).xyz;
        const vec4 mr_map = sample_texture_default_bias(mr_tex, verts.uv0, tex_lod, vec4(1.0));
    #else
        vec4 color = mat_data.color;
        vec3 N = vec3(0.5, 0.5, 1.0);
        const vec4 mr_map = vec4(1.0);
    #endif

    const vec3 hit_loc = (gl_WorldRayOriginEXT + gl_WorldRayDirectionEXT * gl_HitTEXT) 
        + (N * 0.02);
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
    F0 = mix(F0, color.rgb, metallic);

    vec4 brdf = compute_brdf_pdf(sun_dir, N, V, color.rgb, F0, roughness, metallic);

    // Ambient lighting
    const vec3 kS = specular_f_roughness(max(dot(N, V), 0.0), F0, roughness);
    const vec3 kD = (1.0 - metallic) * (vec3(1.0) - kS);

    const vec3 diffuse = color.rgb
        * texture(di_map, N).rgb 
        * global_lighting.ambient_color_intensity.rgb;
    const vec3 ambient = global_lighting.ambient_color_intensity.a * kD * diffuse;

    hit_value.emissive = vec4(ambient, 0.0);
    hit_value.brdf = vec4(brdf.rgb, 1.0);
    hit_value.location = vec4(hit_loc, 1.0);
    hit_value.hit = 1;
}