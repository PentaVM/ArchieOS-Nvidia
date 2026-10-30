// Decorative envelope at the existing field radius. Uniform model scale keeps
// the normal transform exact; the view direction uses the active native camera.
VARYING vec3 atmosphereNormal;
VARYING vec3 atmosphereView;

void MAIN()
{
    vec4 world = MODEL_MATRIX * vec4(VERTEX, 1.0);
    atmosphereNormal = normalize(mat3(MODEL_MATRIX) * NORMAL);
    atmosphereView = CAMERA_POSITION - world.xyz;
    POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
}
