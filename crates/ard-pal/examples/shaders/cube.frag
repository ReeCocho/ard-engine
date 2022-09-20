#version 450 core

layout(location = 0) out vec4 OUT_COLOR;
layout(location = 0) in vec2 IN_UV;

layout(set = 0, binding = 0) uniform sampler2D tex;

void main() {
    OUT_COLOR = vec4(texture(tex, IN_UV).rgb, 1.0);
}