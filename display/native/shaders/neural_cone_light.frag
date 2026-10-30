VARYING float nsV;

void MAIN()
{
    float coverage = pow(1.0 - nsV, 3.0) * 0.055;
    FRAGCOLOR = vec4(vec3(0.006, 0.055, 0.11) * coverage, coverage);
}
