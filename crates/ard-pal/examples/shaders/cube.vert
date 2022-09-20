#version 450 core

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec2 UV;

layout(location = 0) out vec2 OUT_UV;

layout(set = 0, binding = 1) uniform UBO {
    mat4 MVP;
};

void main() {
    gl_Position = MVP * POSITION;
    OUT_UV = UV;
}