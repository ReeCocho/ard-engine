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
    int[TABLE_Z][TABLE_X][TABLE_Y] light_counts;
    uint[TABLE_Z][TABLE_X][TABLE_Y][MAX_POINT_LIGHTS] clusters; 
};

layout(set = 2, binding = 0) uniform CameraUBO {
    mat4 view;
    mat4 projection;
    mat4 vp;
    mat4 view_inv;
    mat4 projection_inv;
    mat4 vp_inv;
    vec4[6] planes;
    vec4 properties;
    vec4 position;
    vec2 scale_bias;
} camera;

const vec3 SLICE_TO_COLOR[TABLE_Z] = vec3[TABLE_Z](
    vec3(0.1),
    vec3(1.0),
    vec3(0.5),
    vec3(1.0, 0.0, 0.0),
    vec3(0.0, 1.0, 0.0),
    vec3(0.0, 0.0, 1.0),
    vec3(1.0, 1.0, 0.0),
    vec3(1.0, 0.0, 1.0),
    vec3(0.0, 1.0, 1.0),
    vec3(0.5, 0.0, 0.0),
    vec3(0.0, 0.5, 0.0),
    vec3(0.0, 0.0, 0.5),
    vec3(0.5, 0.5, 0.0),
    vec3(0.5, 0.0, 0.5),
    vec3(0.0, 0.5, 0.5),
    vec3(0.9)
);

void main() {
    // Determine which cluster the fragment is in
    vec2 uv = ((FRAG_POS.xy / FRAG_POS.w) * 0.5) + vec2(0.5);

    ivec3 cluster = ivec3(
        clamp(int(uv.x * float(TABLE_X)), 0, TABLE_X - 1),
        clamp(int(uv.y * float(TABLE_Y)), 0, TABLE_Y - 1),
        clamp(int(log(FRAG_POS.z) * camera.scale_bias.x - camera.scale_bias.y), 0, TABLE_Z - 1)
    );

    int count = light_counts[cluster.z][cluster.x][cluster.y];
    
    FRAGMENT_COLOR = vec4(0.1, 0.1, 0.1, 1.0);
    
    // FRAGMENT_COLOR += vec4(float(count) * 0.005);
    for (int i = 0; i < count; i++) {
        PointLight light = lights[clusters[cluster.z][cluster.x][cluster.y][i]];
        float l = length(light.position_range.xyz - WORLD_POS);
        if (l < light.position_range.w) {
            FRAGMENT_COLOR += (1.0 - ((l * l) / (light.position_range.w * light.position_range.w))) * vec4(light.color_intensity.w);
        }
    }
}