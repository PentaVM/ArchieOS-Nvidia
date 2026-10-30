// pulses.ts PULSE_VERT: the signals that travel along the links. UV0.x is the fade.
VARYING float nsFade;
VARYING vec3 nsColour;

void MAIN()
{
    nsFade = UV0.x;
    nsColour = NORMAL;
    POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
    POINT_SIZE = (2.0 + 2.6 * UV0.x) * pixelRatio;
}
