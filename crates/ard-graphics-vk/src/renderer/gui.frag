#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

layout(location = 0) out vec4 OUT_COLOR;

layout(location = 0) in vec2 UV;
layout(location = 1) in vec4 COLOR;

layout(set = 0, binding = 0) uniform sampler2D FONT_ATLAS;
layout(set = 1, binding = 0) uniform sampler2D[] ARD_TEXTURES;

layout(push_constant) uniform constants {
    vec2 scale;
	vec2 translate;
    uint texture_idx;
};

void main() {
    vec4 color = COLOR;
    
    if (texture_idx == 4294967295) {
        color *= texture(FONT_ATLAS, UV).r;
    } else {
        color *= texture(ARD_TEXTURES[texture_idx], UV);
    }

    OUT_COLOR = color;
}