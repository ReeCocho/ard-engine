#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec3 LOCAL_POS;

layout(set = 0, binding = 1) uniform samplerCube sky_box;

void main() {
    const vec3 color = texture(sky_box, normalize(LOCAL_POS)).rgb;
    FRAGMENT_COLOR = vec4(color, 1.0);
}