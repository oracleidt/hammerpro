# Native system angular-speed constants

Nine authored Garry's Mod systems isolate the root `rotation_speed` value,
strict DMX types and initializer precedence. The constant is raw radians per
second; Rotation Speed Random's constant uses degrees per second. Both root
float pi and initializer constant 180 therefore produce the same readout.

The final `Remap Scalar` operator copies rotation-speed attribute 5 into
position-X attribute 0 using identical input/output ranges [-256,256]. The
effect's native render bounds expose X. This measures an initialized constant
without depending on frame duration or inferring speed from a rotating image.
The remap itself has a separate 900-case native arithmetic reference. Its f32
range arithmetic slightly quantizes pi; the recorded values are its actual
native output, rather than assumed mathematical pi.

| Case | Input | Native encoded X |
|---|---|---|
| none | Root absent | 0 |
| pi | Root float pi | 3.1416015625 |
| npi | Root float -pi | -3.1415863037109375 |
| p180 | Root float 180 | 180 |
| root0_init180 | Root 0, initializer constant 180 degrees/s | 3.1416015625 |
| root180_init0 | Root 180, initializer constant 0 | 0 |
| p180_int | Wrong root type: int 180 | 0 |
| p180_string | Wrong root type: string "180" | 0 |
| p1_bool | Wrong root type: bool true | 0 |

Each system emits one opaque white particle, radius 24, initially at (0,0,64).
The included material uses the authored four-quadrant 8×8 SpriteCard texture.
The camera follows native encoded X, at Y=-128 and Z=64, with FOV 60 and a
128×128 viewport. Capture uses point-sampled RGBA16161616F, tone mapping 1,
then UnlitGeneric sRGB readback. No installed assets or binaries are changed.

The regression runs the actual PCF parser, simulation, UI adapter/sorter and
GPU. It first requires **exact f32 bits** for all nine encoded-X values, then
compares every RGB pixel against the native PNG (tolerance 3/255). No particle
attributes are injected. Before the fix, pi/-pi/180 read back as zero and their
images differed by 255 RGB levels because the particle no longer matched the
native tracking camera.

This fixture establishes the constant and initializer replacement. General
root-parameter typing, other constants and frame-by-frame angular integration
are not inferred from these static readouts; existing rotation-operator tests
cover their own native cases separately.
