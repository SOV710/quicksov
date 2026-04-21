#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float itemWidth;
    float itemHeight;
    float fromCenterX;
    float toCenterX;
    float progress;
    float activeHalfWidth;
    float activeHalfHeight;
    float mergeStrength;
    vec4 blobColor;
} ubuf;

float sdCircle(vec2 p, vec2 c, float r) {
    return length(p - c) - r;
}

float sdCapsule(vec2 p, vec2 a, vec2 b, float r) {
    vec2 pa = p - a;
    vec2 ba = b - a;
    float h = clamp(dot(pa, ba) / max(dot(ba, ba), 0.0001), 0.0, 1.0);
    return length(pa - ba * h) - r;
}

float sdRoundBox(vec2 p, vec2 c, vec2 halfSize, float r) {
    vec2 q = abs(p - c) - halfSize + vec2(r);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

float smin(float a, float b, float k) {
    float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

void main() {
    vec2 px = qt_TexCoord0 * vec2(ubuf.itemWidth, ubuf.itemHeight);

    float p = clamp(ubuf.progress, 0.0, 1.0);
    float cy = ubuf.itemHeight * 0.5;
    float fromX = ubuf.fromCenterX;
    float toX = ubuf.toCenterX;
    float movingX = mix(fromX, toX, p);
    float distance = abs(toX - fromX);
    float motion = smoothstep(1.0, 4.0, distance);
    float tail = sin(p * 3.14159265) * motion;

    float r = ubuf.activeHalfHeight;
    float k = max(ubuf.mergeStrength, 0.001);
    vec2 moving = vec2(movingX, cy);
    vec2 source = vec2(fromX, cy);
    vec2 target = vec2(toX, cy);

    float d = sdRoundBox(
        px,
        moving,
        vec2(ubuf.activeHalfWidth + r * 0.20 * tail, ubuf.activeHalfHeight),
        r
    );

    float sourceKeep = 1.0 - smoothstep(0.12, 0.78, p);
    float sourceRadius = r * sourceKeep;
    d = smin(d, sdCircle(px, source, sourceRadius), max(k * sourceKeep, 0.001));

    float targetKeep = smoothstep(0.34, 1.0, p);
    float targetHalfWidth = mix(r, ubuf.activeHalfWidth, targetKeep);
    d = smin(
        d,
        sdRoundBox(px, target, vec2(targetHalfWidth, ubuf.activeHalfHeight), r),
        max(k * targetKeep, 0.001)
    );

    float sourceBridgeRadius = r * 0.72 * tail * (1.0 - smoothstep(0.76, 1.0, p));
    d = smin(d, sdCapsule(px, source, moving, sourceBridgeRadius), max(k * tail, 0.001));

    float targetBridgeRadius = r * 0.48 * tail * smoothstep(0.42, 1.0, p);
    d = smin(d, sdCapsule(px, moving, target, targetBridgeRadius), max(k * tail, 0.001));

    float aa = max(fwidth(d), 0.75);
    float alpha = (1.0 - smoothstep(-aa, aa, d)) * ubuf.blobColor.a * ubuf.qt_Opacity;
    fragColor = vec4(ubuf.blobColor.rgb * alpha, alpha);
}
