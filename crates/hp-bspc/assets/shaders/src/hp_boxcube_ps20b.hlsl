sampler MaskSampler : register(s0);
sampler EnvSampler : register(s1);

const float4 cBoxMin : register(c0);
const float4 cBoxMax : register(c1);
const float4 cOrigin : register(c2);
const float4 cTint : register(c3);
const float3 cEyePos : register(c10);
const float4 cLightScale : register(c30);

struct PS_INPUT
{
    float2 uv : TEXCOORD0;
    float3 world : TEXCOORD1;
    float3 normal : TEXCOORD2;
    float fade : TEXCOORD3;
};

float4 main(PS_INPUT i) : COLOR
{
    float3 n = normalize(i.normal);
    float3 v = normalize(cEyePos - i.world);
    float3 r = reflect(-v, n);
    float3 dir = r;
    if (cTint.w > 0.5)
    {
        float3 safe = r + (abs(r) < 1e-5) * 1e-5;
        float3 inv = 1.0 / safe;
        float3 t1 = (cBoxMax.xyz - i.world) * inv;
        float3 t2 = (cBoxMin.xyz - i.world) * inv;
        float3 tf = max(t1, t2);
        float d = min(min(tf.x, tf.y), tf.z);
        dir = i.world + r * max(d, 0.0) - cOrigin.xyz;
    }
    float3 spec = texCUBE(EnvSampler, dir).rgb;
    spec = lerp(spec, spec * spec, cOrigin.w);
    float4 m = tex2D(MaskSampler, i.uv);
    float mode = cBoxMin.w;
    float mask = 1.0;
    float3 mask3 = float3(1.0, 1.0, 1.0);
    if (mode > 2.5) mask3 = m.rgb;
    else if (mode > 1.5) mask = m.a;
    else if (mode > 0.5) mask = 1.0 - m.a;
    float f = cBoxMax.w + (1.0 - cBoxMax.w) * pow(1.0 - saturate(dot(n, v)), 5.0);
    float3 c = spec * cTint.rgb * mask * mask3 * f * cLightScale.x * i.fade;
    return float4(c, 1.0);
}
