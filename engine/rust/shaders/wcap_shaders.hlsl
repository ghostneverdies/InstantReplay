#pragma warning (disable : 3571)

SamplerState LinearSampler : register(s0);

static float3 LinearToGamma(float3 Color)
{
	return Color < 0.0031308 ? Color * 12.92 : 1.055 * pow(Color, 1.0 / 2.4) - 0.055;
}

static float3 GammaToLinear(float3 Color)
{
	return Color < 0.04045 ? Color / 12.92 : pow((Color + 0.055) / 1.055, 2.4);
}

static uint PackToBGR(float3 Color)
{
	const uint3 Pack = { 1, 1 << 8, 1 << 16 };
	return dot(uint3(saturate(Color.bgr) * 255 + 0.5), Pack);
}

static uint LinearPackToBGR(float3 Color)
{
	return PackToBGR(LinearToGamma(Color));
}

Texture2D<float3> ResizeIn  : register(t0);
RWTexture2D<uint> ResizeOut : register(u0);

static float ResizeFilter(float x)
{

	x = abs(x);

	if (x < 1.0)
	{
		float x2 = x * x;
		float x3 = x * x2;
		x = (21 * x3 - 36 * x2 + 16) / 18;
	}
	else if (x < 2.0)
	{
		float x2 = x * x;
		float x3 = x * x2;
		x = (-7 * x3 + 36 * x2 - 60 * x + 32) / 18;
	}
	else
	{
		x = 0.0;
	}

	return x;
}

static float3 ResizePass(uint2 OutputPos, int2 Direction)
{
	int2 InSize, OutSize;
	ResizeIn.GetDimensions(InSize.x, InSize.y);
	ResizeOut.GetDimensions(OutSize.x, OutSize.y);

	float2 Scale = float2(OutSize) / InSize;
	float2 Size = 2.0 / Scale;

	float2 Center = float2(OutputPos) + 0.5;

	float2 InputPos = Center / Scale;
	int2 StartPos = clamp(int2(InputPos - Size + 0.5), 0, InSize - 1);
	int2 EndPos = clamp(int2(InputPos + Size + 0.5), 0, InSize - 1);

	float Start = dot(Direction, StartPos) + 0.5;
	float End = dot(Direction, EndPos) + 0.5;
	int2 Pos = StartPos * Direction.xy + int2(InputPos - 0.5) * Direction.yx;

	float WeightSum = 0;
	float3 ColorSum = 0;

	for (float Index = Start; Index < End; Index += 1.0)
	{
		float Offset = dot(Direction, Center - Index * Direction * Scale);
		float Weight = ResizeFilter(Offset);

		ColorSum += Weight * ResizeIn[Pos];
		WeightSum += Weight;
		Pos += Direction;
	}

	return ColorSum / WeightSum;
}

[numthreads(16, 16, 1)]
void ResizePassH(uint3 OutputPos: SV_DispatchThreadID)
{
	ResizeOut[OutputPos.xy] = PackToBGR(ResizePass(OutputPos.xy, int2(1, 0)));
}

[numthreads(16, 16, 1)]
void ResizePassV(uint3 OutputPos: SV_DispatchThreadID)
{
	ResizeOut[OutputPos.xy] = PackToBGR(ResizePass(OutputPos.xy, int2(0, 1)));
}

[numthreads(16, 16, 1)]
void ResizeLinearPassH(uint3 OutputPos: SV_DispatchThreadID)
{
	ResizeOut[OutputPos.xy] = LinearPackToBGR(ResizePass(OutputPos.xy, int2(1, 0)));
}

[numthreads(16, 16, 1)]
void ResizeLinearPassV(uint3 OutputPos: SV_DispatchThreadID)
{
	ResizeOut[OutputPos.xy] = LinearPackToBGR(ResizePass(OutputPos.xy, int2(0, 1)));
}

Texture2D<float3>  ConvertIn   : register(t0);
Texture2D<float2>  ConvertInUV : register(t1);

RWTexture2D<unorm float>  ConvertOutY  : register(u0);
RWTexture2D<unorm float2> ConvertOutUV : register(u1);

cbuffer ConvertMatrix : register(b0)
{
	row_major float3x3 RGB_To_YUV;
	row_major float3x3 YUV_To_RGB;
}

static const float RANGE_Y  = 219.0 / 255.0;
static const float RANGE_UV = 224.0 / 255.0;
static const float OFFSET_Y = 16.0 / 255.0;
static const float OFFSET_UV = 0.5 / 255.0 + 0.5;

static const float3 LumaWeights = float3(0.2126, 0.7152, 0.0722);

static float LumaFromRGB(float3 LinearColor)
{
	return dot(LumaWeights, LinearColor);
}

static float RgbToY(float3 Color)
{
	return dot(RGB_To_YUV[0], Color);
}

static float2 RgbToUV(float3 Color)
{
	return mul(RGB_To_YUV, Color).yz;
}

[numthreads(16, 16, 1)]
void ConvertSinglePass(uint3 OutputPos: SV_DispatchThreadID)
{
	uint4 Pos4 = OutputPos.xyxy * 2 + uint4(0, 0, 1, 1);

	float2 InSize;
	ConvertIn.GetDimensions(InSize.x, InSize.y);

	float2 ColorPos = float2(Pos4.xy) / InSize;
	float3 Color0 = ConvertIn.SampleLevel(LinearSampler, ColorPos, 0, int2(0, 1));
	float3 Color1 = ConvertIn.SampleLevel(LinearSampler, ColorPos, 0, int2(1, 1));
	float3 Color = lerp(Color0, Color1, 0.5);

	ConvertOutUV[OutputPos.xy] = RgbToUV(Color) * RANGE_UV + OFFSET_UV;

	ConvertOutY[Pos4.xy] = RgbToY(ConvertIn[Pos4.xy]) * RANGE_Y + OFFSET_Y;
	ConvertOutY[Pos4.zy] = RgbToY(ConvertIn[Pos4.zy]) * RANGE_Y + OFFSET_Y;
	ConvertOutY[Pos4.xw] = RgbToY(ConvertIn[Pos4.xw]) * RANGE_Y + OFFSET_Y;
	ConvertOutY[Pos4.zw] = RgbToY(ConvertIn[Pos4.zw]) * RANGE_Y + OFFSET_Y;
}

[numthreads(16, 16, 1)]
void ConvertPass1(uint3 OutputPos: SV_DispatchThreadID)
{
	uint2 Pos2 = OutputPos.xy * 2;

	float2 InSize;
	ConvertIn.GetDimensions(InSize.x, InSize.y);

	float2 ColorPos = float2(Pos2) / InSize;
	float3 Color0 = ConvertIn.SampleLevel(LinearSampler, ColorPos, 0, int2(0, 1));
	float3 Color1 = ConvertIn.SampleLevel(LinearSampler, ColorPos, 0, int2(1, 1));
	float3 Color = lerp(Color0, Color1, 0.5);

	ConvertOutUV[OutputPos.xy] = RgbToUV(Color) * RANGE_UV + OFFSET_UV;
}

[numthreads(16, 16, 1)]
void ConvertPass2(uint3 Pos: SV_DispatchThreadID)
{
	float2 InSize;
	ConvertInUV.GetDimensions(InSize.x, InSize.y);

	float2 PosBase = float2(Pos.xy / 2);
	float2 PosOffset = (Pos.xy & 1) ? float2(1.0, 0.75) : float2(0.5, 0.25);
	float2 UV = ConvertInUV.SampleLevel(LinearSampler, (PosBase + PosOffset) / InSize, 0);

	float3 Color = ConvertIn[Pos.xy];

	float L = LumaFromRGB(Color*Color);
	float3 W = LumaWeights;
	float3 K = mul(YUV_To_RGB, float3(0, UV * (1.0 / RANGE_UV) - (OFFSET_UV / RANGE_UV)));
	float A = dot(W, K);
	float B = dot(W, K*K);
	float C = A*A - B + L;
	float Y = sqrt(C) - A;

	ConvertOutY[Pos.xy] = saturate(Y) * RANGE_Y + OFFSET_Y;
}
