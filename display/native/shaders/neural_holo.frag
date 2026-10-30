#version 440
// The neural sphere's hologram pass (scene.ts HOLO_FRAG): a small red/blue split, scan
// lines, a slow bright band and a vignette, over the bloomed frame. `strength` is the
// original's `holo` setting.

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float strength;
    float lightTheme;
    vec2 resolution;
};
layout(binding = 1) uniform sampler2D source;

void main()
{
    vec2 uv = qt_TexCoord0;
    if (lightTheme > 0.5) {
        fragColor = texture(source, uv) * qt_Opacity;
        return;
    }
    float ca = 0.0007;
    vec3 c = vec3(texture(source, uv + vec2(ca, 0.0)).r, texture(source, uv).g, texture(source, uv - vec2(ca, 0.0)).b);
    float scan = mix(1.0, 0.94 + 0.06 * sin(uv.y * resolution.y * 1.5708), strength);
    float band = 1.0 + 0.06 * strength * smoothstep(0.035, 0.0, abs(fract(uv.y - time * 0.05) - 0.5));
    float vig = smoothstep(1.2, 0.4, length((uv - 0.5) * vec2(resolution.x / resolution.y, 1.0)));
    fragColor = vec4(c * scan * band * vig, 1.0) * qt_Opacity;
}
