#version 450 core

layout(location = 0) out float FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

layout(binding = 0) uniform sampler2D in_image;

layout(push_constant) uniform block {
    ivec2 image_size;
};

void main() {
    vec4 texels;
    texels.x = textureOffset(in_image, UV, ivec2(0, 0)).x;
    texels.y = textureOffset(in_image, UV, ivec2(-1, 0)).x;
    texels.z = textureOffset(in_image, UV, ivec2(-1, -1)).x;
    texels.w = textureOffset(in_image, UV, ivec2(0, -1)).x;

    float maxZ = max(max(texels.x, texels.y), max(texels.z, texels.w));

    vec3 extra;

    // if we are reducing an odd-width texture then fetch the edge texels
    if (((image_size.x & 1) != 0) && (int(gl_FragCoord.x) == image_size.x-3)) {
        // if both edges are odd, fetch the top-left corner texel
        if (((image_size.y & 1) != 0) && (int(gl_FragCoord.y) == image_size.y-3)) {
            extra.z = textureOffset(in_image, UV, ivec2(1, 1)).x;
            maxZ = max(maxZ, extra.z);
        }

        extra.x = textureOffset(in_image, UV, ivec2(1,  0)).x;
        extra.y = textureOffset(in_image, UV, ivec2(1, -1)).x;
        maxZ = max(maxZ, max(extra.x, extra.y));
    } else
    // if we are reducing an odd-height texture then fetch the edge texels
    if (((image_size.y & 1) != 0) && (int(gl_FragCoord.y) == image_size.y-3)) {
        extra.x = textureOffset(in_image, UV, ivec2( 0, 1)).x;
        extra.y = textureOffset(in_image, UV, ivec2(-1, 1)).x;
        maxZ = max(maxZ, max(extra.x, extra.y));
    }

    FRAGMENT_COLOR = maxZ;
}