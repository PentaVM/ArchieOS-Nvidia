// The neural sphere's lines and constant-colour particles (three.js LineBasicMaterial and
// PointsMaterial with fog): a colour per vertex or one tint, times the brightness, pulled
// towards the background by the original's linear fog.
VARYING vec3 nsColour;
VARYING float nsFog;

void MAIN()
{
    POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
    vec4 view = VIEW_MATRIX * (MODEL_MATRIX * vec4(VERTEX, 1.0));
    nsFog = smoothstep(fogNear, fogFar, -view.z);
    nsColour = mix(tint, NORMAL, vertexColours) * brightness;
    POINT_SIZE = pointSize;
}
