#version 450 core

#include "data_structures.glsl"

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 COLOR;

layout(location = 0) out vec4 VERT_COLOR;

layout(set = 0, binding = 0) uniform ARD_Camera {
    Camera camera;
};

void main() {
    gl_Position = camera.vp * vec4(POSITION.xyz, 1.0);
    VERT_COLOR = COLOR;
}