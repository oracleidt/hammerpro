# Native particle ordering within renderer passes

Two selected Garry's Mod captures isolate a collection containing two particles
and either one or two identical SpriteCard renderers. The authored particles
are red at (10,0,128) and blue at (10,32,128), both radius 24 and alpha 128/255.
The camera is (10,-128,128), yaw 90, FOV 60, with a 128×128 viewport. The native
bounds confirm the local maxima (10,32,64); the effect origin adds Z=64.

The native center is (188,0,137) with one renderer and (207,0,152) with two:
far blue, near red, then far blue, near red. Globally sorting all four instances
changes the sequence to blue, blue, red, red and the preview differed by up to
32 RGB levels. Sorting must happen within each authored renderer pass.

The test parses and simulates the unmodified PCF, including `Remap Particle
Count to Scalar` and both `Remap Scalar to Vector` initializers. It asserts the
resulting particle positions and colors against the native values before
passing them through the real adapter, sorter and GPU. No attributes are
injected. These initializers also have separate batch arithmetic and live
precedence references; see `native_initializer_order.md` and the hp-pcf
initializer-remapping reference notes.

The generator/probe also contain exploratory root sorting-flag variants. Their
semantics are not asserted here: the committed reference selects only the two
default cases. The test's scope is ordering inside one effect, not the native
priority of multiple independent effect instances in a game scene.

PNG capture uses the same point-sampled RGBA16161616F, tone mapping 1 and sRGB
readback as the other particle references. RGB comparisons cover the entire
image with a tolerance of 3/255. The GMA contains only the authored white texture
and SpriteCard material. Installed game assets and binaries are unchanged.
