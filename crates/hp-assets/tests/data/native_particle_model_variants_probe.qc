$modelname "hp_modelanim117/probe.mdl"
$surfaceprop "metal"
$cdmaterials "hp_modelanim117"
$bodygroup "variant"
{
 studio "probe.smd"
 studio "offset.smd"
}
$bodygroup "extra"
{
 blank
 studio "small.smd"
}
$sequence "idle" "probe.smd" fps 1
$sequence "slide" "slide.smd" fps 30 loop activity ACT_IDLE 1
$texturegroup "skinfamilies"
{
 { "gray" }
 { "red" }
}
