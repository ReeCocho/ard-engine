#define LINE_IS_OOB 2

uint epipolar_line_base(uint idx) {
    return consts.sample_count_per_line * idx;
}

uint epipolar_line_sample(uint line, uint s) {
    return epipolar_line_base(line) + s;
}

#ifdef HAS_GLOBAL_LIGHTING
vec2 get_sun_uv() {
    const vec4 sun_ndc = camera.vp * vec4(global_lighting.sun_direction.xyz, 0.0);
    vec2 sun_uv = (vec2(sun_ndc.xy / sun_ndc.w) + vec2(1.0)) * 0.5;
    return sun_uv;
}
#endif

vec2 epipolar_line_edge_uv(uint line) {
    const float line_idx_norm = 4.0 * float(line) / float(consts.line_count);
    // These formula create a path that moves clockwise around the UV square starting at (0, 1)
    return vec2(
        clamp(1.5 - abs(line_idx_norm - 1.5), 0.0, 1.0),
        clamp(abs(line_idx_norm - 2.5) - 0.5, 0.0, 1.0)
    );
}

vec2 project_uv_to_edge(vec2 start, vec2 pt) {
    const vec3 startv3 = vec3(start, 0.0);
    const vec3 start_to_pt = vec3(pt, 0.0) - startv3; 
    const vec3 v_pt = normalize(start_to_pt);
    const vec3 v_top_left = normalize(vec3(0.0, 1.0, 0.0) - startv3);
    const vec3 v_top_right = normalize(vec3(1.0, 1.0, 0.0) - startv3);
    const vec3 v_bot_left = normalize(vec3(0.0, 0.0, 0.0) - startv3);
    const vec3 v_bot_right = normalize(vec3(1.0, 0.0, 0.0) - startv3);

    const bool top_intersect = cross(v_top_left, v_pt).z <= 0.0 
        && cross(v_pt, v_top_right).z <= 0.0;

    const bool bot_intersect = cross(v_bot_right, v_pt).z <= 0.0 
        && cross(v_pt, v_bot_left).z <= 0.0;

    const bool right_intersect = cross(v_top_right, v_pt).z <= 0.0 
        && cross(v_pt, v_bot_right).z <= 0.0;

    // Finding the intersection points involves solving one of four systems of equations
    // depending on the intersection side:
    //
    // Top:
    // pt.x + t*v_pt.x = x
    // pt.y + t*v_pt.y = 1
    // Solve for x
    //
    // Right:
    // pt.x + t*v_pt.x = 1
    // pt.y + t*v_pt.y = y
    // Solve for y
    //
    // Bottom:
    // pt.x + t*v_pt.x = x
    // pt.y + t*v_pt.y = 0
    // Solve for x
    //
    // Left:
    // pt.x + t*v_pt.x = 0
    // pt.y + t*v_pt.y = y
    // Solve for y

    const float mixer = float(top_intersect || bot_intersect);

    const vec2 t_eq_params = mix(
        vec2(pt.x, v_pt.x),
        vec2(pt.y, v_pt.y),
        mixer
    );

    const vec2 var_eq_params = mix(
        vec2(pt.y, v_pt.y),
        vec2(pt.x, v_pt.x),
        mixer
    );

    const float c = float(top_intersect || right_intersect);

    const float t = (c - t_eq_params.x) / t_eq_params.y;
    const float var = var_eq_params.x + (t * var_eq_params.y);

    const vec2 eq1 = vec2(float(right_intersect), var);
    const vec2 eq2 = vec2(var, float(top_intersect));

    return mix(eq1, eq2, float(top_intersect || bot_intersect));
    
    /*
    // Find the point for the intersecting side where the ray intersects
    if (top_intersect) {
        
        const float t = (1.0 - pt.y) / v_pt.y;
        const float x = pt.x + (t * v_pt.x);
        return vec2(x, 1.0);
    }
    else if (right_intersect) {
        
        const float t = (1.0 - pt.x) / v_pt.x;
        const float y = pt.y + (t * v_pt.y);
        return vec2(1.0, y);
    }
    else if (bot_intersect) {
        const float t = (-pt.y) / v_pt.y;
        const float x = pt.x + (t * v_pt.x);
        return vec2(x, 0.0);
    }
    // Left intersect
    else {

        const float t = (-pt.x) / v_pt.x;
        const float y = pt.y + (t * v_pt.y);
        return vec2(0.0, y);
    }
    */
}

vec2 sample_to_uv(uint line, uint s, vec2 sun_uv) {
    const vec2 line_edge_uv = epipolar_line_edge_uv(line);

    // If the sun is OOB, we first project it to be on the edge of the screen
    if (sun_uv.x < 0.0 || sun_uv.x > 1.0 || sun_uv.y < 0.0 || sun_uv.y > 1.0) {
        sun_uv = project_uv_to_edge(line_edge_uv, sun_uv);
    }

    return clamp(
        mix(sun_uv, line_edge_uv, float(s) / float(consts.sample_count_per_line - 1)),
        0.0,
        1.0
    );
}

bool is_initial_sample(uint line, uint s) {
    return 
        // Always sample first and last sample points
        s == 0 
        || s == consts.sample_count_per_line - 1
        // Initial sampling
        || s % uint(float(consts.sample_count_per_line) / float(consts.initial_sample_count)) == 0
        // Minimum low samples at every other sample
        || ((s & 1) == 1 && s <= consts.low_sample_minimum * 2);
}