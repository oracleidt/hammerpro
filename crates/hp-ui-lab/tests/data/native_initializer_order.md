# Native initializer precedence and output

Ten actual Garry's Mod captures check `Remap Particle Count to Scalar` and
`Remap Scalar to Vector` together with ordinary radius, position and color
initializers. Each authored PCF emits four particles. All cases use their
actual PCF simulation in the GPU regression, without attribute injection.

| Case | Native result |
|---|---|
| count | radii 1, 2, 3, 4, exported to position X |
| radius_count | ordinary radius 7, then remapping gives 1..4 |
| count_radius | remapping still runs after the base radius initializer |
| radius_count_mul | existing radius 7 multiplied by 1..4, maximum 28 |
| vector | position (10,20,94), including control-point Z=64 |
| box_vector | remapping replaces the ordinary box position |
| vector_box | remapping still runs after the base box initializer |
| box_vector_mul | (7,8,73) × (10,20,94) = (70,160,6862) |
| color_vector | remapping replaces constant gray with (0.5,0.25,1) |
| color_vector_mul | constant gray multiplied component by component |

Both remappers are modifiers: base attribute initializers run first, followed
by modifiers in authored order. The count remapper keeps a separate counter
per initializer instance; it does not use the number of currently alive
particles. The vector remapper updates previous position as well as position,
so stationary output does not accidentally acquire velocity.

For eight cases the final scalar remapping operator exports the radius or
position X. The color cases display their actual color without that diagnostic
operator. The committed JSON includes native position X/Z extremes; all ten
must match simulation float bits exactly. Every GPU RGB pixel is compared with
the native PNG, tolerance 3/255. A white particle renders the authored four-color
RGBA8888 texture through a SpriteCard material.

The camera follows the captured extrema, at Y −128, yaw 90, FOV 60, 128×128.
Capture uses point-sampled RGBA16161616F, tone mapping 1 and sRGB readback.
The PCF, authored GMA, Rust generator and public-API Lua probe reproduce these
cases without changing installed game assets or binaries.

The separate hp-pcf numeric references cover restricted counters, endpoint
clamps, equal/reversed ranges, creation-time windows, current-value
multiplication, control-point interpolation, and local matrix transforms.
These images establish this static authored setup, not every particle
attribute, rendering operator or collision mode.
