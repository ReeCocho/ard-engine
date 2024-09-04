#version 450 core
#extension GL_EXT_scalar_block_layout : enable
#extension GL_EXT_nonuniform_qualifier : enable
#extension GL_EXT_control_flow_attributes : enable

#define ARD_SET_LXAA 0
#include "ard_bindings.glsl"

const float LXAA_EDGE_THRES = 0.5;
const float LXAA_OFF_MIN = 0.0625;
const float LXAA_PROP_MAIN = 0.4;
const float LXAA_PROP_EACH_NP = 0.3;

#define GetLuma(c) dot(c, vec3(0.299, 0.587, 0.114))
#define SampleColor(p) texture(src, p).rgb
#define SampleLumaOff(p, o) GetLuma(textureOffset(src, p, o).rgb)

const ivec2 OffSW = ivec2(-1, 1);
const ivec2 OffSE = ivec2( 1, 1);
const ivec2 OffNE = ivec2( 1,-1);
const ivec2 OffNW = ivec2(-1,-1);

layout(push_constant) uniform constants {
    LxaaPushConstants consts;
};

layout(location = 0) in vec2 tco;
layout(location = 0) out vec4 out_color;

void main() {
    const vec3 defColor = SampleColor(tco);
    const vec2 RCP2 = vec2(consts.screen_dims) * vec2(2.0);

    const vec4 lumaA = vec4(
        SampleLumaOff(tco, OffSW),
        SampleLumaOff(tco, OffSE),
        SampleLumaOff(tco, OffNE),
        SampleLumaOff(tco, OffNW)
    );

    const float gradientSWNE = lumaA.x - lumaA.z;
    const float gradientSENW = lumaA.y - lumaA.w;

    const vec2 dir = vec2(
        gradientSWNE + gradientSENW,
        gradientSWNE - gradientSENW
    );

    const vec2 dirM = abs(dir);
    const float lumaAMax = max(max(lumaA.x, lumaA.y), max(lumaA.z, lumaA.w));
    const float localLumaFactor = lumaAMax * 0.5 + 0.5;
    const float localThres = LXAA_EDGE_THRES * localLumaFactor;
    const bool lowDelta = abs(dirM.x - dirM.y) < localThres;

    if (lowDelta) {
        out_color = vec4(defColor, 1.0);
        return;
    }

    const float dirMMin = min(dirM.x, dirM.y);
    const vec2 offM = clamp(LXAA_OFF_MIN * dirM / dirMMin, 0.0, 1.0);
    vec2 offMult = RCP2 * sin(dir);

    const float offMMax = max(offM.x, offM.y);
    if (offMMax == 1.0) {
        const bool horSpan = offM.x == 1.0;
		const bool negSpan = horSpan ? offMult.x < 0 : offMult.y < 0;
		const bool sowSpan = horSpan == negSpan;
		vec2 tcoC = tco;
		if( horSpan) tcoC.x += 2.0 * offMult.x;
		if(!horSpan) tcoC.y += 2.0 * offMult.y;

		vec4 lumaAC = lumaA;
		if( sowSpan) lumaAC.x = SampleLumaOff(tcoC, OffSW);
		if(!negSpan) lumaAC.y = SampleLumaOff(tcoC, OffSE);
		if(!sowSpan) lumaAC.z = SampleLumaOff(tcoC, OffNE);
		if( negSpan) lumaAC.w = SampleLumaOff(tcoC, OffNW);

		const float gradientSWNEC = lumaAC.x - lumaAC.z;
		const float gradientSENWC = lumaAC.y - lumaAC.w;
		vec2 dirC = vec2(
            gradientSWNEC - gradientSENWC,
            gradientSWNEC + gradientSENWC
        );

		if(!horSpan) dirC = dirC.yx;
		const bool passC = abs(dirC.x) > 2.0 * abs(dirC.y);
		if(passC) offMult *= 2.0;
    }

    const vec2 offset = offM * offMult;

	const vec3 rgbM = SampleColor(tco);
	const vec3 rgbN = SampleColor(tco - offset);
	const vec3 rgbP = SampleColor(tco + offset);
	const vec3 rgbR = (rgbN + rgbP) * LXAA_PROP_EACH_NP + rgbM * LXAA_PROP_MAIN;

	const float lumaR = GetLuma(rgbR);
	const float lumaAMin = min(min(lumaA.x, lumaA.y), min(lumaA.z, lumaA.w));
	const bool outOfRange = (lumaR < lumaAMin) || (lumaR > lumaAMax);
    const vec3 finColor = mix(rgbR, defColor, float(outOfRange));

    out_color = vec4(finColor, 1.0);
}