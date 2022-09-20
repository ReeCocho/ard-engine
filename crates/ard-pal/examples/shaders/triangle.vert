#version 450 core

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 COLOR;

layout(location = 0) out vec4 OUT_COLOR;

void main() {
    gl_Position = POSITION;
    OUT_COLOR = COLOR;
}