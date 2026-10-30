// Light atmosphere: teal Fresnel rim and a softer external halo. RGB is
// premultiplied by coverage for One / OneMinusSrcAlpha composition.
VARYING vec3 atmosphereNormal;
VARYING vec3 atmosphereView;

void MAIN()
{
    vec3 normal = normalize(atmosphereNormal);
    float facing = clamp(abs(dot(normal, normalize(atmosphereView))), 0.0, 1.0);
    float radial = sqrt(max(0.0, 1.0 - facing * facing));
    vec3 ink;
    float coverage;
    if (atmospherePass > 0.5) {
        // Envelope radius is 1.08 R. Fade to zero at its silhouette, so this
        // remains a soft halo instead of drawing a second hard sphere outline.
        float band = smoothstep(0.80, 0.92, radial) * (1.0 - smoothstep(0.92, 1.0, radial));
        coverage = 0.16 * band;
        ink = vec3(0.012, 0.23, 0.31);
    } else {
        float edge = pow(1.0 - facing, 2.4);
        float rim = pow(1.0 - facing, 8.0);
        // A stylized depth cue, not a physical illumination or telemetry value.
        float light = 0.35 + 0.65 * max(dot(normal, normalize(vec3(-0.42, 0.62, 0.66))), 0.0);
        vec3 volume = mix(vec3(0.022, 0.075, 0.14), vec3(0.055, 0.20, 0.29), light);
        ink = mix(volume, vec3(0.008, 0.18, 0.23), edge);
        coverage = 0.10 + 0.32 * edge + 0.14 * rim;
    }
    FRAGCOLOR = vec4(ink * coverage, coverage);
}
