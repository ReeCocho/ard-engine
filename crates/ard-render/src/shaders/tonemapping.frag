#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

const float GAMMA = 2.2;

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

layout(set = 0, binding = 0) uniform sampler2D screen_tex;

layout(push_constant) uniform constants {
    vec2 screen_size;
    float exposure;
	bool fxaa_enabled;
};

void main() {
    vec3 color = texture(screen_tex, UV).rgb;
    color = vec3(1.0) - exp(-color * exposure);
    color = pow(color, vec3(1.0 / GAMMA));

    FRAGMENT_COLOR = vec4(color, 1.0);
}