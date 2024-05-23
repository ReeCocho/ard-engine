#ifndef _ARD_PBR_COMMON_RT
#define _ARD_PBR_COMMON_RT

// Random number generation using pcg32i_random_t, using inc = 1. Our random state is a uint.
uint step_rng(uint rng_state) {
    return rng_state * 747796405 + 1;
}

// Steps the RNG and returns a floating-point value between 0 and 1 inclusive.
float rng_float(inout uint rng_state) {
    // Condensed version of pcg_output_rxs_m_xs_32_32, with simple conversion to floating-point [0,1].
    rng_state = step_rng(rng_state);
    uint word = ((rng_state >> ((rng_state >> 28) + 4)) ^ rng_state) * 277803737;
    word = (word >> 22) ^ word;
    return float(word) / 4294967295.0;
}

#endif