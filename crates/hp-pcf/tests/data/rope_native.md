# Native rope vertex builders

`rope_gpu_native.json` records 24 executions of the installed native `C_OP_RenderRope::RenderSpriteCard` routine at client RVA 0x44e140. `rope_cpu_native.json` records the same 24 parameter sets through native Render at RVA 0x44db90 and its rope mesh builder. Each batch emits 14 vertices: 672 native vertices in total, or 336 distinct point V coordinates. The source DLL SHA256 is embedded in both files.

The probe executes the native routines offline in Unicorn, using owned attribute buffers, mesh streams and minimal renderer API callbacks for allocation/binding/locking/drawing. Geometry, color conversion, texture coordinates, subdivision and control-point arithmetic run as native instructions. The CPU mesh builder’s RSQRTSS uses the host x86 instruction through the same small helper used by the native trail references; no approximate reciprocal formula replaces it. The native mesh-builder stream reset also runs. The installed game process and files are not patched.

The fixture uses texel sizes 1/4/8, all eight control-point scaling combinations, texture height 16, scroll 2.5, offset 0.25 and time 1.25. Native CPU/GPU paths have intentionally different UV semantics, including offset/scroll behavior. The core regression compares all 336 distinct V coordinates by float bits.
