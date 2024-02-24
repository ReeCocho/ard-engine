vec3 skybox_color(vec3 pos, vec3 sun_dir) {
    const float Br = 0.0025;
    const float Bm = 0.0003;
    const float g =  0.9800;
    const vec3 nitrogen = vec3(0.650, 0.570, 0.475);
    const vec3 Kr = Br / pow(nitrogen, vec3(4.0));
    const vec3 Km = Bm / pow(nitrogen, vec3(0.84));

    // Discard to default color below the horizon
    if (pos.y < 0.0) {
        return vec3(0.408, 0.388, 0.373) * 0.25;
    }

    // Atmosphere Scattering
    const vec3 fsun = -sun_dir;

    const float mu = dot(normalize(pos), normalize(fsun));
    const float rayleigh = 3.0 / (8.0 * 3.14) * (1.0 + mu * mu);
    const vec3 mie = (Kr + Km * (1.0 - g * g) / (2.0 + g * g) / pow(1.0 + g * g - 2.0 * g * mu, 1.5)) / (Br + Bm);

    const vec3 day_extinction = exp(-exp(-((pos.y + fsun.y * 4.0) * (exp(-pos.y * 16.0) + 0.1) / 80.0) / Br) * (exp(-pos.y * 16.0) + 0.1) * Kr / Br) * exp(-pos.y * exp(-pos.y * 8.0 ) * 4.0) * exp(-pos.y * 2.0) * 4.0;
    const vec3 night_extinction = vec3(1.0 - exp(fsun.y)) * 0.2;
    const vec3 extinction = mix(day_extinction, night_extinction, -fsun.y * 0.2 + 0.5);

    return rayleigh * mie * extinction;
}