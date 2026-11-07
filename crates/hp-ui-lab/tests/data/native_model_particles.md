# Native model-particle references

The fixture contains an owned, asymmetric 12-triangle model, its SMD/QC source, a PCF, a GMA and eleven 128 x 128 captures from the unmodified Garry's Mod client. No installed game assets are redistributed. The source client SHA-256 and native bounds/vertex count are recorded in JSON. The additional error/stock controls in JSON disambiguate the valid model from the Source ERROR fallback; those game models are not included in the GMA.

The native factory is `Render models`. Its `sequence 0 model` filename is relative to `models/`: `hp_model117/probe.mdl`. The engine adds the prefix unconditionally. The original full-path investigation rendered the ERROR model and is not used as a geometry reference.

The eleven cases cover radius 0.5/1/2, roll, yaw, normal X/Y/Z, default normal with orient-Z enabled, gray modulation and alpha. Each actual PCF is simulated without attribute injection, then rendered through both the viewport model-particle buffer and the particle-preview path. Native captures use suppressed engine lighting and a white ambient cube; comparisons consequently use Unshaded mode, no MSAA, matching camera and the Source pixel grid. Ordinary Textured mode retains editor lighting. All twenty-two comparisons passed with maximum RGB delta 0/255 after retaining authored VTX winding instead of reorienting faces from shading normals.

The model deliberately has non-geometric shading normals. This catches a pre-existing renderer failure: `pack_triangles` adjusted model winding from those normals, making some views completely disappear. World-brush winding repair remains separate; model winding comes from VTX.

A first yaw capture differed in direction despite the same serialized file. A fresh capture from an owned native process was checked read-only: the native inherited flip byte was zero and yaw initial was +90 degrees / +pi/2 radians. That fresh image is the stored yaw reference. No native attributes or memory were written. Other ten images were unchanged. This reference establishes the non-flipped initializer result, rather than claiming that all native random/runtime state is identical between launches.

Reproduce model compilation by copying the SMD/QC into an owned source directory and running StudioMDL with an owned `-game` directory. The fixture generator accepts output directory and compiled-model game directory as its two arguments. The supplied Lua mounts its own GMA, loads its own PCF and captures into `data/`.
