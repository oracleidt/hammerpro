# Native initializer remapping references

`initializer_count_native.json` records 960 native batches (4,791 particles).
`initializer_vector_native.json` records 1,200 batches (4,794 particles).
Each file identifies the installed Garry's Mod client DLL by SHA-256 and the
native entry point. The references were obtained by executing its unmodified
x64 instructions in Unicorn, including the actual control-point sampling and
matrix helpers; no arithmetic or helper was substituted.

The unit regressions require bit-exact scalar/vector results. Count batches
also require the exact final persistent counter. Vector position output also
requires the exact previous position. Unaligned starts and multiple SIMD
storage blocks are represented in the native inputs.

Count cases include all currently represented scalar fields, integer input
ranges, equal/reversed endpoints, restricted operation, existing-value
multiplication, and counters across the float precision boundary. Alpha
endpoints are clamped during parameter initialization; results are not clamped
again after multiplication. A restricted counter that was already above its
maximum at the start of a batch stops advancing; skipped particles inside a
batch still advance it.

Vector cases cover position, previous position, color and hitbox-relative
position. Input is the trail-length scalar. Cases include creation times on
and outside both window edges, either unspecified window endpoint, equal and
reversed ranges, component multiplication, CP 0/1/63, current/previous control
point locations and local affine frames. The time window uses particle birth
time and has an exclusive end; either endpoint −1 disables the window.

Position mapping always includes the time-sampled control-point location,
including when local coordinates are disabled. Local coordinates use current
control-point axes and the native matrix helper's operation grouping. With
local multiplication, previous position becomes the resulting current
position; with nonlocal multiplication, previous position is multiplied
independently.

These are arithmetic and state references for the represented fields. They
do not establish unsupported scalar attributes, normal vectors, malformed
DMX types, or global engine scheduling. Actual game captures separately check
modifier precedence, positions, colors and the complete GPU path in
hp-ui-lab's `native_initializer_order` and `native_render_order` fixtures.
