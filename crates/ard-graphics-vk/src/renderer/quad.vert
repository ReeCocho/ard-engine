#version 450 core

layout(location = 0) out vec2 UV;

vec4 positions[3] = vec4[](
    vec4(-1.0, 1.0, 0.0, 1.0),
    vec4(-1.0, 3.0, 0.0, 1.0),
    vec4(3.0, 1.0, 0.0, 1.0)
);

vec2 uvs[3] = vec2[](
    vec2(0.0, 0.0),
    vec2(0.0, 2.0),
    vec2(2.0, 0.0)
);

void main() {
    UV = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    gl_Position = vec4(UV * 2.0 + -1.0, 0.0, 1.0);
}