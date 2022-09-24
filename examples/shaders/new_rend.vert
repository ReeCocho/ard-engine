#version 450 core

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 COLOR;

layout(location = 1) out vec4 VERT_COLOR;

void main() {
    gl_Position = POSITION;
    VERT_COLOR = COLOR;
}
