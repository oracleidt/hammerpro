# Native prop material conversion reference

Owned fixtures captured in the installed Garry's Mod renderer on 2026-11-08.
The client DLL SHA256, native shader names, tint readbacks, all 64 RGB samples
per material, and PNG hashes are in report.json. No game assets were modified.
The owned game process 10616 was closed and the three temporary Lua overrides
were restored by the capture runner.

source.vmt is the original LightmappedGeneric. converted.vmt is the actual
output of hp_ui_mapping::propper::prepare_material_files, with the accompanying
owned black texture and floating-point HDR cubemap. before.png used the old
converter's unchanged tint .1. after.png used the public converter output's
.3511192 tint. capture.lua mounts the owned data/hp_conv117_assets.gma and draws
all three real file materials through the native material system, not a shader
reimplementation. The archive contains these source textures and the textures
and VMT relocated to models/hp_conv117/probe, plus the old VMT as hp_conv117/before.

HDR level 2, tone scale 1, 128x128 target. Original sample RGB170,63,29;
old conversion45,11,3; corrected171,64,29. Maximum whole-image RGB difference
is125 before and1 after. Tests couple the complete staged VMT to this native
capture, and verify the native image comparison. This proves this static tint
conversion; it does not assert equivalence of arbitrary shader features or
material proxies between brush and model shaders.

The separate native UV routing reference in hp-assets/tests/data establishes
that brush detail samples the base transform while VertexLitGeneric detail
samples its own detail transform. The converter now routes those matrices and
adapts shader override names without modifying already-valid model materials.
