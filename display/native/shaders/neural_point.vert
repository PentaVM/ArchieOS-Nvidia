// The neural sphere's node dots (scene.ts POINT_VERT). Per vertex: position, colour in the
// normal channel, UV0 = (kind, fire), UV1 = (ring, born). Size, alpha and colour are the
// original's: a node that fires pulses, a running program breathes, depth fades it, and
// `born` ramps it in while the sphere builds.
VARYING vec3 nsColour;
VARYING float nsAlpha;
VARYING float nsRing;

void MAIN()
{
    float kind = UV0.x;
    float fireSeed = UV0.y;
    float ring = UV1.x;
    float born = UV1.y;
    vec4 view = VIEW_MATRIX * (MODEL_MATRIX * vec4(VERTEX, 1.0));
    POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
    float fire = fireSeed > 0.0 ? pow(max(0.0, sin(time * 0.8 + fireSeed * 61.0)), 40.0) : 0.0;
    float proc = kind > 1.5 ? 0.5 + 0.5 * sin(time * 2.4) : 0.0;
    nsRing = ring;
    POINT_SIZE = (ring > 0.5 ? 7.0 : (kind > 0.5 ? 3.2 : 2.7)) * pixelRatio * (1.0 + fire * 0.5 + proc * 0.25) * nodeSize * born;
    float depth = 1.0 - smoothstep(nearDistance, farDistance, -view.z);
    nsAlpha = ((kind > 0.5 ? 0.95 : 0.8) * (0.3 + 0.7 * depth) + fire * 0.7) * born;
    nsColour = NORMAL * (1.0 + fire * 2.0 + proc * 0.6) * brightness;
}
