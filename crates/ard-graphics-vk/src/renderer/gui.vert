#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

layout(location = 0) in vec2 POSITION;
layout(location = 1) in vec2 UV;
layout(location = 2) in vec4 COLOR;

layout(location = 0) out vec2 OUT_UV;
layout(location = 1) out vec4 OUT_COLOR;

layout(push_constant) uniform constants {
    vec2 scale;
	vec2 translate;
    uint texture_idx;
};

void main() {
    OUT_UV = UV;
    OUT_COLOR = COLOR;
    gl_Position = vec4(POSITION * scale + translate, 0.0, 1.0);
}