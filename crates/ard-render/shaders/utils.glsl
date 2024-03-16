vec2 oct_wrap(const vec2 v) {
    return (1.0 - abs(v.yx)) * mix(vec2(-1.0), vec2(1.0), greaterThanEqual(v.xy, vec2(0.0)));
}

vec2 oct_encode(vec3 n) {
    n /= abs(n.x) + abs(n.y) + abs(n.z);
    n.xy = n.z >= 0.0 ? n.xy : oct_wrap(n.xy);
    n.xy = (n.xy * 0.5) + vec2(0.5);
    return n.xy;
}

vec3 oct_decode(vec2 f) {
    f = (f * 2.0) - vec2(1.0);
    vec3 n = vec3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
    vec2 t = vec2(clamp(-n.z, 0.0, 1.0));
    n.xy += mix(t, -t, greaterThanEqual(n.xy, vec2(0.0)));
    return normalize(n);
}

vec2 view_encode(vec3 n) {
    return n.xy * 0.5 + vec2(0.5);
}

vec3 view_decode(vec2 f) {
    vec3 n = vec3(f * 2 - vec2(1.0), 0.0);
    n.z = -sqrt(1.0 + dot(n.xy, -n.xy));
    return normalize(n);
}