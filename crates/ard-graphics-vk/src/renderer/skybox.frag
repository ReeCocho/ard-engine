#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec3 LOCAL_POS;

layout(set = 0, binding = 7) uniform sampler2D equirectuangular_map;

const vec2 inv_atan = vec2(0.1591, 0.3183);

vec2 sample_spherical_map(vec3 v) {
    vec2 uv = vec2(atan(v.z, v.x), asin(v.y));
    uv *= inv_atan;
    uv += 0.5;
    uv.y = 1.0 - uv.y;
    return uv;
}

void main() {
    vec2 uv = sample_spherical_map(normalize(LOCAL_POS));
    vec3 color = texture(equirectuangular_map, uv).rgb;
    color = color / (color + vec3(1.0));
    color = pow(color, vec3(1.0/2.2));
    FRAGMENT_COLOR = vec4(color, 1.0);
}