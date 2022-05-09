#version 450
#extension GL_ARB_separate_shader_objects : enable

#define TABLE_X 32
#define TABLE_Y 16
#define TABLE_Z 16
#define MAX_POINT_LIGHTS 256

struct PointLight {
    vec4 color_intensity;
    vec4 position_range;
};

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec4 FRAG_POS;
layout(location = 1) in vec4 IN_COLOR;
layout(location = 2) in vec3 WORLD_POS;
layout(location = 4) flat in uint INSTANCE_IDX;

layout(set = 0, binding = 1) readonly buffer PointLights {
    PointLight[] lights;
};

layout(set = 0, binding = 3) readonly buffer PointLightTable {
    int[TABLE_X][TABLE_Y][TABLE_Z] light_counts;
    uint[TABLE_X][TABLE_Y][TABLE_Z][MAX_POINT_LIGHTS] clusters; 
};

void main() {
    // Determine which cluster the fragment is in
    vec2 uv = (FRAG_POS.xy * 0.5) + vec2(0.5);

    ivec3 cluster = ivec3(
        clamp(int(uv.x * float(TABLE_X)), 0, TABLE_X - 1),
        clamp(int(uv.y * float(TABLE_Y)), 0, TABLE_Y - 1),
        clamp(int(log2(FRAG_POS.z + 1.0) * float(TABLE_Z)), 0, TABLE_Z - 1)
    );

    FRAGMENT_COLOR = vec4(0.1, 0.1, 0.1, 1.0);

    int count = light_counts[cluster.x][cluster.y][cluster.z];
    // FRAGMENT_COLOR += vec4(float(count) * 0.005);
    for (int i = 0; i < count; i++) {
        PointLight light = lights[clusters[cluster.x][cluster.y][cluster.z][i]];
        float l = length(light.position_range.xyz - WORLD_POS);
        if (l < light.position_range.w) {
            FRAGMENT_COLOR += (1.0 - ((l * l) / (light.position_range.w * light.position_range.w))) * vec4(light.color_intensity.w);
        }
    }
}