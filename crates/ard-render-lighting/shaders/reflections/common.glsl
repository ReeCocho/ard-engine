#ifndef _REFLECTIONS_COMMON
#define _REFLECTIONS_COMMON

const uint TILE_SIZE = 8;

const uint RAY_LEN_SCALE_FACT = 1000;

const uint TILE_TYPE_IGNORE = 0;
const uint TILE_TYPE_RT = 1;

#ifndef ARD_SET_REFLECTION_RESET
uvec2 get_tile_dims() {
    return (consts.target_dims + uvec2(TILE_SIZE - 1)) / uvec2(TILE_SIZE);
}

ivec2 get_texel_coord(const uint tile_id) {
    const uint tiles_per_row = (consts.target_dims.x + TILE_SIZE - 1) / TILE_SIZE;
    const uint y = tile_id / tiles_per_row;
    const uint x = tile_id - (tiles_per_row * y);
#ifdef ARD_SET_REFLECTIONS_PASS
    return (ivec2(x, y) * ivec2(TILE_SIZE)) + ivec2(gl_LaunchIDEXT.xy);
#else
    return (ivec2(x, y) * ivec2(TILE_SIZE)) + ivec2(gl_LocalInvocationID.xy);
#endif
}
#endif

#endif