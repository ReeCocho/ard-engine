#version 450 core

layout(location = 0) out vec4 OUT_COLOR;

layout(set = 0, binding = 0) uniform samplerCube environment_map;

layout(location = 0) in vec3 SAMPLE_DIR;

const float PI = 3.14159265359;

void main() {
    const vec3 normal = normalize(SAMPLE_DIR);
    vec3 irradiance = vec3(0.0);

    vec3 up = vec3(0.0, 1.0, 0.0);
    const vec3 right = normalize(cross(up, normal));
    up = normalize(cross(normal, right));

    const float sample_delta = 0.025;
    float nr_samples = 0.0;
    for (float phi = 0.0; phi < 2.0 * PI; phi += sample_delta) {
        for (float theta = 0.0; theta < 0.5 * PI; theta += sample_delta) {
            // Spherical to cartesian (in tangent space)
            const vec3 tangent_sample = vec3(
                sin(theta) * cos(phi), 
                sin(theta) * sin(phi), 
                cos(theta)
            );

            // Tangent space to world space
            const vec3 sample_vec = 
                tangent_sample.x * right + 
                tangent_sample.y * up + 
                tangent_sample.z * normal;

            irradiance += texture(environment_map, sample_vec).rgb * cos(theta) * sin(theta);
            nr_samples += 1.0;
        }
    }

    irradiance = PI * irradiance * (1.0 / float(nr_samples));
    OUT_COLOR = vec4(irradiance, 1.0);
}