# Native screen-velocity rotation reference

`native_screen_velocity.pcf` contains eleven self-authored systems. The matching
generator and Lua probe are included here; authored textures/materials and eleven
128×128 native PNG captures live in `hp-render/tests/data/native_screen_velocity*`.
The game was Garry's Mod 2000.00.01. No installed assets or binaries were changed.

The live probe renders a single 24-unit-radius particle using a four-quadrant
SpriteCard texture. It covers an ordinary sprite, the modifier alone, both
renderer orders, rates 0/1/30/3600, forward angles 0/90, initial rotation 30,
zero motion and motion along the camera's depth axis. Rendering uses a point
sampled RGBA16161616F target, tone mapping 1, then UnlitGeneric sRGB readback.
The JSON retains actual bounds/position, camera and silhouette rows. The helper's
tracking camera intentionally stays at Y=-128 even for depth motion; particle Y
is recovered from the measured bounds rather than assumed to be zero.

Observed native behavior:

- This operator modifies the rotation attribute before other renderers draw,
  regardless of its position in the renderer list. Alone it draws nothing.
- It overwrites rotation, including a nonzero initialized angle, and uses the
  affine view-space current/previous positions without perspective division.
- `forward_angle` is added after atan2. The native default is -90 degrees.
- The native render method does not read `rotate_rate(dps)`; the four tested
  rates produce identical images. Zero screen motion gives atan2(0,0)=0.

`native_screen_velocity_angles.json` separately records 360 native numerical
results for large coordinates, small motions, zero motion, rotated/pitched view
matrices and forward angles. `native_screen_velocity_angles.py --native-math`
regenerates them by executing the installed method's arithmetic in Unicorn,
including its installed scalar atan2 path (CPU dispatch 0), with fresh synthetic
particle attributes. Its DLL SHA-256 and entry points are recorded in the JSON.
This reads an installed DLL into an offline emulator; it does not attach to or
patch the game. The optional UCRT substitution mode is not the committed oracle;
the optimized CPU-dispatch-3 path is not covered by the numerical fixture.

Tests require exact f32 bits for all 360 numerical results and compare all RGB
pixels of the eleven live images with a tolerance of 3/255. Separate adapter
coverage changes the editor camera without advancing simulation and checks the
exported sprite rotation. This is evidence for this operator and these cases,
not a claim that all PCF operators, renderers or effects are identical to Source.

To regenerate live captures, compile the fixture generator against hp-pcf,
hp-gma and hp-vtf; place its output in an isolated test mod's physical data
directory. Run the Lua probe in that mod, preserving any existing autorun files.
Copy only hp_velocity117_reference.json and the eleven hp_velocity117_*.png
outputs. Keep the test game process and authored files separate from user maps.
