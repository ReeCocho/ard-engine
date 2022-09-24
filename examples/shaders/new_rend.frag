#version 450

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 1) in vec4 VERT_COLOR;

void main() {
    FRAGMENT_COLOR = VERT_COLOR;
}
