local report = {cases={}, errors={}}
local effects = {}
local target = GetRenderTargetEx("hp_trail_color117_hdr",128,128,RT_SIZE_NO_CHANGE,MATERIAL_RT_DEPTH_SEPARATE,1,CREATERENDERTARGETFLAGS_HDR,IMAGE_FORMAT_RGBA16161616F)
local readback = GetRenderTarget("hp_trail_color117_readback",128,128)
local blit = CreateMaterial("hp_trail_color117_hdr_copy","UnlitGeneric",{["$basetexture"]=target:GetName(),["$ignorez"]="1",["$nofog"]="1"})
local active = false
local frame = 0
hook.Add("HUDShouldDraw","hp_trail117",function() return false end)
hook.Add("InitPostEntity","hp_trail117",function()
 gui.HideGameUI()
 timer.Simple(1,function()
  report.mounted=game.MountGMA("data/hp_trail_color117.gma")
  if not report.mounted then report.errors[#report.errors+1]="GMA missing" end
  local ok,err=pcall(function() game.AddParticles("data/hp_trail_color117.pcf") end)
  if not ok then report.errors[#report.errors+1]=tostring(err) end
  for _,kind in ipairs({"cpu","spline","splineadd","splineaddself"}) do
   local mat=Material("hp_trail117/"..kind)
   local tex=mat:GetTexture("$basetexture")
   if mat:IsError() or not tex or tex:IsError() then report.errors[#report.errors+1]="bad material "..kind end
   for _,name in ipairs({"default","one","dark","fade","mixed","over","alpha2","low","white","green","alpha1","alpha2byte","alpha3"}) do
    local key=kind.."_"..name
    PrecacheParticleSystem("hp_trail117_"..key)
    local effect=CreateParticleSystemNoEntity("hp_trail117_"..key,Vector(0,0,64),Angle(0,0,0))
    if effect and effect:IsValid() then
     effect:SetShouldDraw(true)
     effect:SetShouldSimulate(true)
     effects[key]=effect
    else report.errors[#report.errors+1]="bad effect "..key end
   end
  end
  report.started=CurTime()
  report.version=VERSIONSTR
  active=true
 end)
end)
hook.Add("PostRender","hp_trail117",function()
 if not active then return end
 frame=frame+1
 if frame~=5 then return end
 active=false
 for name,effect in pairs(effects) do
  local low,high=effect:GetRenderBounds()
  local head=math.abs(low.x)>math.abs(high.x) and low.x or high.x
  local center=Vector(head-32,0,64)
  local kind=string.match(name,"^([^_]+)_")
  local material=Material("hp_trail117/"..kind)
  for _,texture in ipairs({"white","alpha64","rgba128","dark64"}) do
  material:SetTexture("$basetexture","hp_trail117/"..texture)
  material:Recompute()
  render.PushRenderTarget(target)
  render.Clear(0,0,0,255,true,true)
  local tone=render.GetToneMappingScaleLinear()
  render.SetToneMappingScaleLinear(Vector(1,1,1))
  cam.Start3D(center+Vector(0,-128,0),Angle(0,90,0),60,0,0,128,128,1,2000)
  effect:Render()
  cam.End3D()
  render.PopRenderTarget()
  render.PushRenderTarget(readback)
  render.Clear(0,0,0,255,true,true)
  render.SetToneMappingScaleLinear(tone)
  cam.Start2D()
  render.SetMaterial(blit)
  render.DrawScreenQuad()
  render.CapturePixels()
  local row={}
  for x=0,127 do
   local r,g,b,a=render.ReadPixel(x,64)
   row[#row+1]={r,g,b,a}
  end
  local png=render.Capture({format="png",x=0,y=0,w=128,h=128,alpha=false})
  local caseName=name.."-"..texture
  if png then file.Write("hp_trail117_"..caseName..".png",png) end
  report.cases[caseName]={texture=texture,effect=name,head=head,center={center.x,center.y,center.z},low={low.x,low.y,low.z},high={high.x,high.y,high.z},row=row,elapsed=CurTime()-report.started}
  cam.End2D()
  render.PopRenderTarget()
  end
  effect:StopEmissionAndDestroyImmediately()
 end
 file.Write("hp_trail_color117_reference.json",util.TableToJSON(report,true))
 print("HAMMER_PRO_TRAIL_COLOR_COMPLETE")
end)
