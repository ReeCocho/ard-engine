#version 450

#define ARD_FRAGMENT_SHADER
#include "user_shaders.glsl"

layout(location = 0) out uvec2 ENTITY_ID;

void entry() {
    ENTITY_ID = uvec2(
        ARD_OBJECT_INFO[ARD_OBJECT_INDICES[ARD_INSTANCE_IDX]].entity_id,
        ARD_OBJECT_INFO[ARD_OBJECT_INDICES[ARD_INSTANCE_IDX]].entity_ver
    );
}

ARD_ENTRY(entry)