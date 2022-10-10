#version 450
#extension GL_ARB_separate_shader_objects : enable
#extension GL_EXT_nonuniform_qualifier : enable

// Based on this implementation:
// https://github.com/McNopper/OpenGL/blob/master/Example42/shader/fxaa.frag.glsl

layout(location = 0) out vec4 FRAGMENT_COLOR;

layout(location = 0) in vec2 UV;

layout(set = 0, binding = 0) uniform sampler2D screen_tex;

layout(push_constant) uniform constants {
    vec2 screen_size;
    float exposure;
	bool fxaa_enabled;
};

const float LUMA_THRESHOLD = 0.25;
const float MUL_REDUCE = 1.0 / 8.0;
const float MIN_REDUCE = 1.0 / 128.0;
const float MAX_SPAN = 16.0;

void main() {
    vec3 color = texture(screen_tex, UV).rgb;

	if (!fxaa_enabled) {
		FRAGMENT_COLOR = vec4(color, 1.0);
		return;
	}

	vec3 colorNW = textureOffset(screen_tex, UV, ivec2(-1, 1)).rgb;
    vec3 colorNE = textureOffset(screen_tex, UV, ivec2(1, 1)).rgb;
    vec3 colorSW = textureOffset(screen_tex, UV, ivec2(-1, -1)).rgb;
    vec3 colorSE = textureOffset(screen_tex, UV, ivec2(1, -1)).rgb;

	// see http://en.wikipedia.org/wiki/Grayscale
	const vec3 toLuma = vec3(0.299, 0.587, 0.114);
	
	// Convert from RGB to luma.
	float lumaNW = dot(colorNW, toLuma);
	float lumaNE = dot(colorNE, toLuma);
	float lumaSW = dot(colorSW, toLuma);
	float lumaSE = dot(colorSE, toLuma);
	float luma = dot(color, toLuma);

	// Gather minimum and maximum luma.
	float lumaMin = min(luma, min(min(lumaNW, lumaNE), min(lumaSW, lumaSE)));
	float lumaMax = max(luma, max(max(lumaNW, lumaNE), max(lumaSW, lumaSE)));
	
	// If contrast is lower than a maximum threshold ...
	if (lumaMax - lumaMin <= lumaMax * LUMA_THRESHOLD) {
		// ... do no AA and return.
		FRAGMENT_COLOR = vec4(color, 1.0);
		return;
	}  
	
	// Sampling is done along the gradient.
	vec2 samplingDirection;	
	samplingDirection.x = -((lumaNW + lumaNE) - (lumaSW + lumaSE));
    samplingDirection.y =  ((lumaNW + lumaSW) - (lumaNE + lumaSE));
    
    // Sampling step distance depends on the luma: The brighter the sampled texels, the smaller the final sampling step direction.
    // This results, that brighter areas are less blurred/more sharper than dark areas.  
    float samplingDirectionReduce = max((lumaNW + lumaNE + lumaSW + lumaSE) * 0.25 * MUL_REDUCE, MIN_REDUCE);

	// Factor for norming the sampling direction plus adding the brightness influence. 
	float minSamplingDirectionFactor = 1.0 / (min(abs(samplingDirection.x), abs(samplingDirection.y)) + samplingDirectionReduce);
    
    // Calculate final sampling direction vector by reducing, clamping to a range and finally adapting to the texture size. 
    vec2 texel_step = 1.0 / screen_size;
    samplingDirection = clamp(samplingDirection * minSamplingDirectionFactor, vec2(-MAX_SPAN), vec2(MAX_SPAN)) * texel_step;
	
	// Inner samples on the tab.
	vec3 rgbSampleNeg = texture(screen_tex, UV + samplingDirection * (1.0/3.0 - 0.5)).rgb;
	vec3 rgbSamplePos = texture(screen_tex, UV + samplingDirection * (2.0/3.0 - 0.5)).rgb;

	vec3 rgbTwoTab = (rgbSamplePos + rgbSampleNeg) * 0.5;  

	// Outer samples on the tab.
	vec3 rgbSampleNegOuter = texture(screen_tex, UV + samplingDirection * (0.0/3.0 - 0.5)).rgb;
	vec3 rgbSamplePosOuter = texture(screen_tex, UV + samplingDirection * (3.0/3.0 - 0.5)).rgb;
	
	vec3 rgbFourTab = (rgbSamplePosOuter + rgbSampleNegOuter) * 0.25 + rgbTwoTab * 0.5;   
	
	// Calculate luma for checking against the minimum and maximum value.
	float lumaFourTab = dot(rgbFourTab, toLuma);
	
	// Are outer samples of the tab beyond the edge ... 
	if (lumaFourTab < lumaMin || lumaFourTab > lumaMax) {
		// ... yes, so use only two samples.
		FRAGMENT_COLOR = vec4(rgbTwoTab, 1.0); 
	}
	else {
		// ... no, so use four samples. 
		FRAGMENT_COLOR = vec4(rgbFourTab, 1.0);
	}
}