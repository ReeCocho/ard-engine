#ifndef _ARD_PBR_COMMON_TS
#define _ARD_PBR_COMMON_TS

layout(constant_id = 0) const uint TS_INVOCATIONS = MAX_TASK_SHADER_INVOCATIONS;
layout(local_size_x_id = 0, local_size_y_id = 1, local_size_z_id = 2) in;

taskPayloadSharedEXT MsPayload payload;

#endif