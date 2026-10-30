// scene.ts POINT_FRAG: a round soft dot, or for a folded program a ring with a core.
VARYING vec3 nsColour;
VARYING float nsAlpha;
VARYING float nsRing;

void MAIN()
{
    float r = length(gl_PointCoord - 0.5);
    if (r > 0.5)
        discard;
    float a = nsRing > 0.5 ? smoothstep(0.08, 0.0, abs(r - 0.36)) + smoothstep(0.16, 0.0, r) : smoothstep(0.5, 0.0, r);
    FRAGCOLOR = vec4(nsColour * a * nsAlpha, 1.0);
}
