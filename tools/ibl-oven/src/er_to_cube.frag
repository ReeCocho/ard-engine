#version 450 core

layout(location = 0) out vec4 OUT_COLOR;

layout(set = 0, binding = 0) uniform sampler2D equirectangular_map;

layout(location = 0) in vec3 SAMPLE_DIR;

const vec2 inv_atan = vec2(0.1591, 0.3183);
vec2 sample_spherical_map(vec3 v) {
    vec2 uv = vec2(atan(v.z, v.x), asin(v.y));
    uv *= inv_atan;
    uv += vec2(0.5);
    uv.y = 1.0 - uv.y;
    return uv;
}

void main() {
    vec2 uv = sample_spherical_map(normalize(SAMPLE_DIR));
    vec3 color = texture(equirectangular_map, uv).rgb;
    OUT_COLOR = vec4(color, 1.0);
}