local report={cases={},errors={}};local effects={};local frame=0;local active=false
local target=GetRenderTargetEx("hp_capacity117_hdr",128,128,RT_SIZE_NO_CHANGE,MATERIAL_RT_DEPTH_SEPARATE,1,CREATERENDERTARGETFLAGS_HDR,IMAGE_FORMAT_RGBA16161616F)
local readback=GetRenderTarget("hp_capacity117_readback",128,128)
local blit=CreateMaterial("hp_capacity117_copy","UnlitGeneric",{["$basetexture"]=target:GetName(),["$ignorez"]="1",["$nofog"]="1"})
hook.Add("InitPostEntity","hp_capacity117",function()timer.Simple(.5,function()
 gui.HideGameUI();local ok,error=xpcall(function()
 assert(game.MountGMA("data/hp_capacity117.gma"));game.AddParticles("data/hp_capacity117.pcf")
 for _,name in ipairs({"children","deep","count6000","count10000"})do
  PrecacheParticleSystem("hp_capacity117_"..name);local effect=CreateParticleSystemNoEntity("hp_capacity117_"..name,Vector(0,0,64),Angle(0,0,0));assert(effect and effect:IsValid(),name)
  effect:SetControlPointOrientation(0,Vector(1,0,0),Vector(0,-1,0),Vector(0,0,1));effect:SetShouldDraw(false);effect:SetShouldSimulate(true);effects[name]=effect
 end;active=true
 end,debug.traceback);if not ok then report.errors[#report.errors+1]=tostring(error);file.Write("hp_capacity117_reference.json",util.TableToJSON(report,true))end
end)end)
hook.Add("PostRender","hp_capacity117",function()
 if not active then return end;frame=frame+1;if frame<8 then return end;active=false
 local ok,error=xpcall(function()
 for _,name in ipairs({"children","deep","count6000","count10000"})do
  local effect=effects[name];local low,high=effect:GetRenderBounds();local record={name=name,low={low.x,low.y,low.z},high={high.x,high.y,high.z}}
  if name=="children"or name=="deep"then
   local tone=render.GetToneMappingScaleLinear();render.SetToneMappingScaleLinear(Vector(1,1,1));effect:SetShouldDraw(true)
   render.PushRenderTarget(target);render.Clear(0,0,0,255,true,true);cam.Start3D(Vector(0,-256,112),Angle(0,90,0),60,0,0,128,128,1,2000)
   render.SuppressEngineLighting(true);render.ResetModelLighting(1,1,1);effect:Render();render.SuppressEngineLighting(false);cam.End3D();render.PopRenderTarget()
   render.PushRenderTarget(readback);render.Clear(0,0,0,255,true,true);cam.Start2D();render.SetMaterial(blit);render.DrawScreenQuad();render.CapturePixels()
   local count=0;for y=0,127 do for x=0,127 do local r,g,b=render.ReadPixel(x,y);if math.max(r,g,b)>4 then count=count+1 end end end
   record.colored_pixels=count;local png=render.Capture({format="png",x=0,y=0,w=128,h=128,alpha=false});assert(png);file.Write("hp_capacity117_"..name..".png",png)
   cam.End2D();render.PopRenderTarget();render.SetToneMappingScaleLinear(tone)
  end
  report.cases[#report.cases+1]=record;effect:StopEmissionAndDestroyImmediately()
 end
 end,debug.traceback);if not ok then report.errors[#report.errors+1]=tostring(error)end
 report.version=VERSIONSTR;file.Write("hp_capacity117_reference.json",util.TableToJSON(report,true))
end)
