// A readable atmospheric limb against the pale canvas. Same normals/sun and
// geometry as the dark view; this is a theme treatment, not measured radiance.
VARYING vec3 worldNormal;
VARYING vec3 worldPosition;
void MAIN()
{
    vec3 normal=normalize(worldNormal);
    vec3 toEye=normalize(CAMERA_POSITION-worldPosition);
    float rim=pow(1.0-abs(dot(normal,toEye)),4.0);
    float sun=dot(normal,normalize(sunDirection));
    float day=smoothstep(-0.20,0.30,sun);
    float lit=smoothstep(-0.55,0.10,sun);
    vec3 ink=mix(vec3(0.22,0.09,0.035),vec3(0.025,0.12,0.24),day);
    float coverage=rim*(0.28+0.36*lit);
    FRAGCOLOR=vec4(ink*coverage,coverage);
}
