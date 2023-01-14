#version 450 core

layout(location = 0) out vec4 OUT_COLOR;

layout(set = 0, binding = 0) uniform samplerCube environment_map;

layout(location = 0) in vec3 SAMPLE_DIR;

layout(push_constant) uniform constants {
    mat4 unused;
    float roughness;
    uint resolution;
};

const float PI = 3.14159265359;

float radical_inverse_vdc(uint bits) {
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return float(bits) * 2.3283064365386963e-10;
}

vec2 hammersley(uint i, uint N) {
    return vec2(float(i) / float(N), radical_inverse_vdc(i));
}  

vec3 importance_sample_ggx(vec2 xi, vec3 N, float r) {
    const float a = r * r;
	
    const float phi = 2.0 * PI * xi.x;
    const float cos_theta = sqrt((1.0 - xi.y) / (1.0 + (a*a - 1.0) * xi.y));
    const float sin_theta = sqrt(1.0 - cos_theta * cos_theta);
	
    // From spherical coordinates to cartesian coordinates
    const vec3 H = vec3(
        cos(phi) * sin_theta,
        sin(phi) * sin_theta,
        cos_theta
    );
	
    // From tangent-space vector to world-space sample vector
    const vec3 up        = abs(N.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
    const vec3 tangent   = normalize(cross(up, N));
    const vec3 bitangent = cross(N, tangent);
	
    const vec3 sample_vec = tangent * H.x + bitangent * H.y + N * H.z;
    return normalize(sample_vec);
}  

float distribution_GGX(float NdotH, float R) {
    float a = R * R;
    float a2 = a * a;
    float NdotH2 = NdotH * NdotH;

    float nom = a2;
    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return nom / denom;
}

void main() {
    const vec3 N = normalize(SAMPLE_DIR);
    const vec3 R = N;
    const vec3 V = R;

    const uint sample_count = 4096 * 4;
    float total_weight = 0.0;
    vec3 prefiltered_color = vec3(0.0);

    for (uint i = 0; i < sample_count; ++i) {
        vec2 xi = hammersley(i, sample_count);
        vec3 H = importance_sample_ggx(xi, N, roughness);
        const float HdotV = max(dot(H, V), 0.0);
        vec3 L = normalize(2.0 * HdotV * H - V);

        float ndotl = max(dot(N, L), 0.0);
        if (ndotl > 0.0) {
            const float NdotH = max(dot(N, H), 0.0);
            float D = distribution_GGX(NdotH, roughness);
            float pdf = (D * NdotH / (4.0 * HdotV)) + 0.0001;

            float sa_texel = 4.0 * PI / (6.0 * float(resolution) * float(resolution));
            float sa_sample = 1.0 / (float(sample_count) * pdf + 0.0001);
            float mip_level = roughness == 0.0 ? 0.0 : 0.5 * log2(sa_sample / sa_texel);

            prefiltered_color += textureLod(environment_map, L, mip_level).rgb * ndotl;
            total_weight += ndotl;
        }
    }

    prefiltered_color /= total_weight;

    OUT_COLOR = vec4(prefiltered_color, 1.0);
}