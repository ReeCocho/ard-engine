#version 450 core

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

layout(binding = 0) uniform sampler2D in_image;

void main() {
    FRAGMENT_COLOR = texture(in_image, vec2(UV.x, 1.0 - UV.y));
}