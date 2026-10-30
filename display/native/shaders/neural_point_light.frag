// Light surface: category ink, with the original round/ring coverage in alpha.
VARYING vec3 nsColour;
VARYING float nsAlpha;
VARYING float nsRing;

// Decode the existing per-category color channel, then use the readable light inks.
// Only presentation changes: categories/positions still come from the native field.
vec3 categoryInk(vec3 source)
{
    float peak = max(max(source.r, source.g), source.b);
    vec3 unit = source / max(peak, 0.001);
    float best = 100.0;
    vec3 ink = vec3(0.02121901, 0.06847817, 0.14995979);
    float distance;
    distance = dot(unit-vec3(0.16826940, 0.57758044, 1.00000000), unit-vec3(0.16826940, 0.57758044, 1.00000000));
    if (distance < best) { best = distance; ink = vec3(0.00000000, 0.07421357, 0.16826940); }
    distance = dot(unit-vec3(1.00000000, 0.57758044, 0.20507874), unit-vec3(1.00000000, 0.57758044, 0.20507874));
    if (distance < best) { best = distance; ink = vec3(0.16202938, 0.05951124, 0.00000000); }
    distance = dot(unit-vec3(0.12960802, 1.00000000, 0.69194629), unit-vec3(0.12960802, 1.00000000, 0.69194629));
    if (distance < best) { best = distance; ink = vec3(0.00000000, 0.10461648, 0.05951124); }
    distance = dot(unit-vec3(0.63771855, 1.00000000, 0.34953270), unit-vec3(0.63771855, 1.00000000, 0.34953270));
    if (distance < best) { best = distance; ink = vec3(0.09084171, 0.08228271, 0.00000000); }
    distance = dot(unit-vec3(0.56251040, 0.36748735, 1.00000000), unit-vec3(0.56251040, 0.36748735, 1.00000000));
    if (distance < best) { best = distance; ink = vec3(0.16513219, 0.03820437, 0.17144110); }
    distance = dot(unit-vec3(1.00000000, 0.29442582, 0.24675311), unit-vec3(1.00000000, 0.29442582, 0.24675311));
    if (distance < best) { best = distance; ink = vec3(0.26635560, 0.03560131, 0.00334654); }
    distance = dot(unit-vec3(0.78142080, 0.97782218, 1.00000000), unit-vec3(0.78142080, 0.97782218, 1.00000000));
    if (distance < best) { best = distance; ink = vec3(0.03189603, 0.07421357, 0.13843162); }
    distance = dot(unit-vec3(0.86306057, 0.96233084, 1.00000000), unit-vec3(0.86306057, 0.96233084, 1.00000000));
    if (distance < best) { best = distance; ink = vec3(0.02121901, 0.06847817, 0.14995979); }
    distance = dot(unit-vec3(1.00000000, 0.85705390, 0.58129353), unit-vec3(1.00000000, 0.85705390, 0.58129353));
    if (distance < best) { best = distance; ink = vec3(0.00000000, 0.08228271, 0.13286832); }
    distance = dot(unit-vec3(1.00000000, 0.44520119, 0.10224173), unit-vec3(1.00000000, 0.44520119, 0.10224173));
    if (distance < best) { best = distance; ink = vec3(0.00000000, 0.07421357, 0.16826940); }
    return ink;
}

void MAIN()
{
    float r = length(gl_PointCoord - 0.5);
    if (r > 0.5)
        discard;
    float a = nsRing > 0.5 ? smoothstep(0.08, 0.0, abs(r - 0.36)) + smoothstep(0.16, 0.0, r) : smoothstep(0.5, 0.0, r);
    float peak = max(max(nsColour.r, nsColour.g), nsColour.b);
    vec3 ink = categoryInk(nsColour);
    float coverage = clamp(a * nsAlpha * brightness * 1.4, 0.0, 1.0);
    FRAGCOLOR = vec4(ink * coverage, coverage);
}
