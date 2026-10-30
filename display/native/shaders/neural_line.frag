VARYING vec3 nsColour;
VARYING float nsFog;

void MAIN()
{
    FRAGCOLOR = vec4(mix(nsColour, fogColour, nsFog), alpha);
}
