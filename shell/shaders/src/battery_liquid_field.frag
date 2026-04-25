#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float itemWidth;
    float itemHeight;
    float level;
    float phase;
    float frontSoftness;
    float waveAmplitude;
    vec4 fillColor;
    vec4 deepColor;
    vec4 backgroundColor;
    vec4 highlightColor;
    float innerRadius;
} ubuf;

float sdRoundRect(vec2 p, vec2 halfSize, float radius) {
    vec2 q = abs(p) - halfSize + vec2(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - radius;
}

void main() {
    vec2 size = vec2(ubuf.itemWidth, ubuf.itemHeight);
    vec2 uv = qt_TexCoord0;
    vec2 px = uv * size;
    vec2 center = size * 0.5;

    float rectDistance = sdRoundRect(
        px - center,
        max(size * 0.5 - vec2(1.0), vec2(0.0)),
        max(ubuf.innerRadius, 1.0)
    );
    float rectAa = max(fwidth(rectDistance), 0.75);
    float clipMask = 1.0 - smoothstep(0.0, rectAa, rectDistance);

    float t = ubuf.phase * 6.28318530718;
    float y = clamp(uv.y, 0.0, 1.0);
    float edgeSoftness = max(ubuf.frontSoftness, 0.001);
    float wedge = mix(-0.032, 0.040, pow(y, 1.18));
    float carrier = ubuf.waveAmplitude * (
        0.55 * sin(y * 9.0 + t * 0.72) +
        0.25 * sin(y * 20.0 - t * 0.38) +
        0.20 * sin((uv.x + y) * 8.0 + t * 0.55)
    );
    float breathing = edgeSoftness * 0.34 * sin(t * 0.45 + y * 1.4);
    float edge = clamp(ubuf.level + wedge + carrier + breathing, 0.0, 1.10);

    float fillMask = 1.0 - smoothstep(edge - edgeSoftness, edge + edgeSoftness, uv.x);
    float depthMix = smoothstep(0.10, 1.0, uv.x);
    float backgroundDepth = smoothstep(0.0, 1.0, uv.x);
    vec3 backgroundRgb = mix(
        ubuf.backgroundColor.rgb,
        ubuf.backgroundColor.rgb * 0.72,
        backgroundDepth
    );

    vec3 liquidRgb = mix(ubuf.fillColor.rgb, ubuf.deepColor.rgb, depthMix * 0.88);
    float fillAlpha = fillMask * mix(ubuf.fillColor.a, ubuf.deepColor.a, depthMix);

    float frontGlow = fillMask * (1.0 - smoothstep(0.0, edgeSoftness * 2.4, abs(uv.x - edge)));
    float topMask = 1.0 - smoothstep(0.10, 0.72, y);
    float shimmer = 0.5 + 0.5 * sin(t * 1.35 + y * 26.0 + uv.x * 4.0);
    vec3 highlightRgb = ubuf.highlightColor.rgb * ubuf.highlightColor.a * frontGlow * topMask * (0.24 + 0.28 * shimmer);

    vec3 rgb = mix(backgroundRgb, liquidRgb, fillMask);
    rgb += highlightRgb;

    float alpha = max(ubuf.backgroundColor.a, fillAlpha);
    alpha = max(alpha, frontGlow * ubuf.highlightColor.a * 0.32);
    alpha *= clipMask * ubuf.qt_Opacity;

    fragColor = vec4(rgb * alpha, alpha);
}
