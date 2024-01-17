#version 450 core

const vec3 POINTS[] = vec3[3](
    vec3(-1.0, -1.0, 0.0),
    vec3(-1.0, 3.0, 0.0),
    vec3(3.0, -1.0, 0.0)
);

layout(location = 0) out vec2 UV;

void main() {
    vec3 position = POINTS[gl_VertexIndex];
    UV = (position.xy * 0.5) + vec2(0.5);
    UV.y = 1.0 - UV.y;
    gl_Position = vec4(position, 1.0);
}