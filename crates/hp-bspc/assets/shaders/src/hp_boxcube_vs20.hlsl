const float4x4 cModelViewProj : register(c4);
const float4 cFogParams : register(c16);
const float4x3 cModel0 : register(c58);

struct VS_INPUT
{
    float4 pos : POSITION;
    float3 normal : NORMAL0;
    float4 uv : TEXCOORD0;
};

struct VS_OUTPUT
{
    float4 proj : POSITION;
    float fog : FOG;
    float2 uv : TEXCOORD0;
    float3 world : TEXCOORD1;
    float3 normal : TEXCOORD2;
    float fade : TEXCOORD3;
};

VS_OUTPUT main(VS_INPUT v)
{
    VS_OUTPUT o;
    float4 proj = mul(float4(v.pos.xyz, 1.0), cModelViewProj);
    o.proj = proj;
    o.fog = 1.0;
    o.uv = v.uv.xy;
    o.world = mul(float4(v.pos.xyz, 1.0), cModel0);
    o.normal = normalize(mul(v.normal, (float3x3)cModel0));
    o.fade = saturate(max(cFogParams.z, -proj.z * cFogParams.w + cFogParams.x));
    return o;
}
