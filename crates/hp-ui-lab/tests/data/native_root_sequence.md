# Native system sequence constants

Ten Garry's Mod captures exercise the integer system attributes
`sequence_number` and `sequence_number 1`, with valid selections 0, 1 and 2.
The primary initializer is `Sequence Random`; the secondary initializer is
`Sequence Two Random`. Both override the corresponding system constant,
including an explicit initializer value of zero.

The authored 8×8 RGBA8888 VTF contains a real sheet resource with three
one-frame sequences: red, blue and green. Its resource directory and physical
chunks are ordered thumbnail, sheet, image. Each sequence samples inside a
solid-color quadrant. The secondary cases use a SpriteCard with
`$dualsequence 1` and `$sequence_blend_mode 0`.

| Cases | Native position X readout | Native center RGB |
|---|---:|---|
| first_none, first_root1_init0 | 0 | 255, 0, 0 |
| first_one | 1 | 0, 0, 255 |
| first_two, first_root0_init2 | 2 | 0, 255, 0 |
| second_none, second_root1_init0 | 0 | 255, 0, 0 |
| second_one | 1 | 187, 0, 187 |
| second_two, second_root0_init2 | 2 | 187, 187, 0 |

The final `Remap Scalar` operator reads attribute 9 or 13 into position X
with identical input/output ranges −256..256. This exposes the actual stored
scalar independently of the image. The native camera follows that X value at
Y −128, Z 64, yaw 90, FOV 60, rendering at 128×128. Each effect emits one
stationary white particle of radius 24 and lifetime 100 seconds.

The regression parses the actual PCF, simulates it without attribute injection,
reads the GMA's actual VMT and sheet, uses the viewport adapter, and compares
all GPU RGB pixels with the PNGs (tolerance 3/255). All ten scalar readouts
must match the native float bits exactly. Native captures use point-sampled
RGBA16161616F, tone mapping 1, and sRGB readback.

The included Rust generator and Lua probe reproduce the fixtures using only
authored assets and public game APIs. Coverage establishes these valid integer
constants, initializer precedence, and static primary/secondary sheet selection.
It does not establish out-of-range indices, malformed DMX attribute types, or
time-varying animation behavior.
