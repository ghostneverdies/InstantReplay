wcap_shaders.hlsl is vendored UNMODIFIED from mmozeiko/wcap
  https://github.com/mmozeiko/wcap  (commit aa25ccb806d7a6e1c0bfdcca863aabcd8e9badfa)
License: Unlicense / public domain (see wcap's LICENSE).

wcap compiles these compute shaders with fxc.exe at build time. We do not need
fxc or the Windows SDK: encoder.rs embeds the HLSL source (include_str!) and
compiles the entry points it needs at encoder start-up with the system
D3DCompile (d3dcompiler_47.dll, present on Windows 10/11). It takes a few
milliseconds and happens once per recording.

Entry points used:
  ConvertSinglePass            BGRA -> NV12 in one dispatch (default)
  ConvertPass1 + ConvertPass2  higher quality two-pass conversion (IR_IMPROVED_CONVERT=1)
  ResizePassH + ResizePassV    Mitchell-Netravali downscale when the output is smaller than the desktop
