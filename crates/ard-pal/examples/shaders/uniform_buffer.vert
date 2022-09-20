#version 450 core

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 COLOR;

layout(location = 0) out vec4 OUT_COLOR;

layout(binding = 0, set = 0) uniform UBO {
    vec2 offset;
};

void main() {
    gl_Position = vec4(POSITION.xy + offset, 0.0, 1.0);
    OUT_COLOR = COLOR;
}