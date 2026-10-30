// scene.ts CONE_VERT: the projector's beam; UV0.y is the cylinder's v.
VARYING float nsV;

void MAIN()
{
    nsV = UV0.y;
    POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
}
