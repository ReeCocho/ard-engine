#version 450 core

layout(location = 0) out vec3 OUT_POSITION;

layout(push_constant) uniform constants {
    mat4 vp;
    float unused;
};

const vec3 POINTS[] = vec3[36](
    // East
    vec3(1.0, -1.0,  1.0),
    vec3(1.0,  1.0,  1.0),
    vec3(1.0,  1.0, -1.0),
    
    vec3(1.0, -1.0,  1.0),
    vec3(1.0, -1.0, -1.0),
    vec3(1.0,  1.0, -1.0),

    // West
    vec3(-1.0, -1.0,  1.0),
    vec3(-1.0,  1.0,  1.0),
    vec3(-1.0,  1.0, -1.0),
    
    vec3(-1.0, -1.0,  1.0),
    vec3(-1.0, -1.0, -1.0),
    vec3(-1.0,  1.0, -1.0),
    // North
    vec3(-1.0, -1.0, 1.0),
    vec3(-1.0,  1.0, 1.0),
    vec3( 1.0,  1.0, 1.0),

    vec3(-1.0, -1.0, 1.0),
    vec3( 1.0,  1.0, 1.0),
    vec3( 1.0, -1.0, 1.0),
    
    // South
    vec3(-1.0, -1.0, -1.0),
    vec3(-1.0,  1.0, -1.0),
    vec3( 1.0,  1.0, -1.0),

    vec3(-1.0, -1.0, -1.0),
    vec3( 1.0,  1.0, -1.0),
    vec3( 1.0, -1.0, -1.0),
    
    // Top
    vec3( 1.0, 1.0,  1.0),
    vec3(-1.0, 1.0,  1.0),
    vec3(-1.0, 1.0, -1.0),

    vec3( 1.0, 1.0,  1.0),
    vec3(-1.0, 1.0, -1.0),
    vec3( 1.0, 1.0, -1.0),

    // Bottom
    vec3( 1.0, -1.0,  1.0),
    vec3(-1.0, -1.0,  1.0),
    vec3(-1.0, -1.0, -1.0),

    vec3( 1.0, -1.0,  1.0),
    vec3(-1.0, -1.0, -1.0),
    vec3( 1.0, -1.0, -1.0)
);

void main() {
    OUT_POSITION = POINTS[gl_VertexIndex];
    gl_Position = vp * vec4(POINTS[gl_VertexIndex], 1.0);
}