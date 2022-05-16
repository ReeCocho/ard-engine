#version 450 core

layout(location = 0) in vec4 POSITION;
layout(location = 1) in vec4 COLOR;

layout(location = 0) out vec4 VERT_COLOR;

layout(set = 0, binding = 0) uniform CameraUBO {
    mat4 view;
    mat4 projection;
    mat4 vp;
    mat4 view_inv;
    mat4 projection_inv;
    mat4 vp_inv;
    vec4[6] planes;
    vec4 properties;
    vec4 position;
    vec2 scale_bias;
} camera;

void main() {
    gl_Position = camera.vp * vec4(POSITION.xyz, 1.0);
    VERT_COLOR = COLOR;
}