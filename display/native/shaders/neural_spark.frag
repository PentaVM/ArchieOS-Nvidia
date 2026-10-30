// pulses.ts PULSE_FRAG
VARYING float nsFade;
VARYING vec3 nsColour;

void MAIN()
{
    float r = length(gl_PointCoord - 0.5);
    if (r > 0.5)
        discard;
    FRAGCOLOR = vec4(mix(nsColour, vec3(1.0), 0.55) * smoothstep(0.5, 0.0, r) * nsFade * 1.5 * brightness, 1.0);
}
