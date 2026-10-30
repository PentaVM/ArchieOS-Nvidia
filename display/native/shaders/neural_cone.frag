// scene.ts CONE_FRAG
VARYING float nsV;

void MAIN()
{
    FRAGCOLOR = vec4(vec3(0.37, 0.83, 1.0) * pow(1.0 - nsV, 3.0) * 0.016, 1.0);
}
