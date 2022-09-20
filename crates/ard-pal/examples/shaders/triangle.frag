#version 450 core

layout(location = 0) out vec4 OUT_COLOR;
layout(location = 0) in vec4 IN_COLOR;

void main() {
    OUT_COLOR = IN_COLOR;
}