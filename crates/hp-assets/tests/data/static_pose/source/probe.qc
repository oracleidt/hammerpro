$modelname "hp_staticpose/probe.mdl"
$surfaceprop "metal"
$cdmaterials "hp_staticpose"
$body "body" "probe.smd"
$sequence "fold" "fold.smd" fps 10 loop
$sequence "reference" "probe.smd" fps 1
