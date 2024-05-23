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

float luminance(vec3 color) {
    return clamp(dot(color, vec3(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
}

void main() {
    const uint object_id = gl_InstanceCustomIndexEXT;
    const uint meshlet_idx = gl_GeometryIndexEXT;
    const uint mesh_id = uint(object_data[object_id].mesh);
    const uint textures_slot = uint(object_data[object_id].textures);
    const mat3 normal_mat = mat3(transpose(object_data[object_id].model_inv));
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
    vec3 tangents[3];
    vec3 bitangents[3];
    [[unroll]]
    for (uint i = 0; i < 3; i++) {
        uint index = v_indices[index_offset + index_base + i];

        const uvec2 ard_normal_raw = v_normals[index];
        const uvec2 ard_tangent_raw = v_tangents[index];

        vec4 ard_normal = vec4(
            unpackSnorm2x16(ard_normal_raw.x),
            unpackSnorm2x16(ard_normal_raw.y)
        );
        vec4 ard_tangent = vec4(
            unpackSnorm2x16(ard_tangent_raw.x),
            unpackSnorm2x16(ard_tangent_raw.y)
        );

        vec3 tangentW = normalize(normal_mat * ard_tangent.xyz);
        vec3 normalW = normalize(normal_mat * ard_normal.xyz);
        tangentW = normalize(tangentW - dot(tangentW, normalW) * normalW);
        vec3 bitangentW = normalize(cross(normalW, tangentW) * ard_tangent.w);

        uvs[i] = unpackHalf2x16(v_uv0s[index]);
        tangents[i] = tangentW;
        bitangents[i] = bitangentW;
        normals[i] = normalW;
    }

    // Compute barycentrics
    const vec3 bc = vec3(1.0 - attribs.x - attribs.y, attribs.x, attribs.y);

    // Compute final UV from barycentrics
    const vec2 uv = uvs[0] * bc.x + uvs[1] * bc.y + uvs[2] * bc.z;

    // Sample textures
    const vec4 color = sample_texture_default(color_tex, uv, vec4(1.0)) * mat_data.color;
    vec3 N = sample_texture_default(normal_tex, uv, vec4(0.5, 0.5, 1.0, 0.0)).xyz;
    const vec4 mr_map = sample_texture_default(mr_tex, uv, vec4(1.0));

    // TBN for normal mapping
    const vec3 tangent = normalize(tangents[0] * bc.x + tangents[1] * bc.y + tangents[2] * bc.z);
    const vec3 bitangent = normalize(bitangents[0] * bc.x + bitangents[1] * bc.y + bitangents[2] * bc.z);
    const vec3 normal = normalize(normals[0] * bc.x + normals[1] * bc.y + normals[2] * bc.z);
    mat3 tbn = mat3(
        normalize(tangent),
        normalize(bitangent),
        normalize(normal)
    );

    // Compute surface normal
    const vec3 V = normalize(gl_WorldRayDirectionEXT);

    // Compute an ortho-normal basis from the surface normal
    vec3 T = abs(normal.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
    T = normalize(cross(normal, T));
    vec3 B = -normalize(cross(normal, T));
    tbn = mat3(T, B, normal);

    N = (N * 2.0) - vec3(1.0);
    N = normalize(tbn * N);

    // Surface properties
    const float metallic = clamp(mat_data.metallic * mr_map.b, 0.0, 1.0);
    const float roughness = clamp(mat_data.roughness * mr_map.g, 0.0, 1.0);

    hit_value.hit = 1;
    hit_value.color = vec4(color.rgb, 1.0);
    hit_value.location = vec4(gl_WorldRayOriginEXT + gl_WorldRayDirectionEXT * gl_HitTEXT, 1.0);
    hit_value.roughness = roughness;
    hit_value.metallic = metallic;
    hit_value.normal = vec4(normal.xyz, 0.0);
    hit_value.tangent = vec4(T, 0.0);
    hit_value.bitangent = vec4(B, 0.0);
}