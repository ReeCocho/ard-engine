#version 450 core

layout(location = 0) out vec4 OUT_COLOR;
layout(location = 0) in vec4 COLOR;

void main() {
    OUT_COLOR = COLOR;
}