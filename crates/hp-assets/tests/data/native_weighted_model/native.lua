local report={cases={},errors={}}
local effects={}
local target=GetRenderTargetEx("hp_weighted117_hdr",128,128,RT_SIZE_NO_CHANGE,MATERIAL_RT_DEPTH_SEPARATE,1,CREATERENDERTARGETFLAGS_HDR,IMAGE_FORMAT_RGBA16161616F)
local readback=GetRenderTarget("hp_weighted117_readback",128,128)
local blit=CreateMaterial("hp_weighted117_copy","UnlitGeneric",{["$basetexture"]=target:GetName(),["$ignorez"]="1",["$nofog"]="1"})
local active=false
local frame=0
hook.Add("CalcView","hp_weighted117",function()return{origin=Vector(0,-128,64),angles=Angle(0,90,0),fov=60,drawviewer=false}end)
hook.Add("HUDShouldDraw","hp_weighted117",function()return false end)
hook.Add("InitPostEntity","hp_weighted117",function()
 gui.HideGameUI()
 timer.Simple(1,function()
  report.mounted=game.MountGMA("data/hp_weighted117.gma")
  if not report.mounted then report.errors[#report.errors+1]="missing authored GMA" end
  util.PrecacheModel("models/hp_modelweight117/probe.mdl")
  local warm=ClientsideModel("models/hp_modelweight117/probe.mdl",RENDERGROUP_OPAQUE);if IsValid(warm)then warm:SelectWeightedSequence(ACT_IDLE);warm:Remove()end
  local ok,err=pcall(function()game.AddParticles("data/hp_weighted117.pcf")end)
  if not ok then report.errors[#report.errors+1]=tostring(err)end
  local model="models/hp_modelweight117/probe.mdl"
  report.model={exists=file.Exists(model,"GAME")}
  local entity=ClientsideModel(model,RENDERGROUP_OPAQUE)
  if IsValid(entity)then report.choices={};for i=1,512 do local n=entity:SelectWeightedSequence(ACT_IDLE);report.choices[tostring(n)]=(report.choices[tostring(n)]or 0)+1 end;local low,high=entity:GetRenderBounds();report.model.low={low.x,low.y,low.z};report.model.high={high.x,high.y,high.z};report.model.materials=entity:GetMaterials();report.model.vertices=0;if util.GetModelMeshes then for _,mesh in ipairs(util.GetModelMeshes(model)or{})do report.model.vertices=report.model.vertices+#mesh.triangles end end;entity:Remove()end
  for _,name in ipairs({"sequence1","sequence2","sequence3","weighted0","weighted1","weighted2","weighted3","weighted4","weighted5","weighted6","weighted7","weighted8","weighted9","weighted10","weighted11","weighted12","weighted13","weighted14","weighted15","weighted16","weighted17","weighted18","weighted19","weighted20","weighted21","weighted22","weighted23","weighted24","weighted25","weighted26","weighted27","weighted28","weighted29","weighted30","weighted31"})do
   PrecacheParticleSystem("hp_weighted117_"..name)
   local effect=CreateParticleSystemNoEntity("hp_weighted117_"..name,Vector(0,0,64),Angle(0,0,0))
   if effect and effect:IsValid()then effect:SetShouldDraw(true);effect:SetShouldSimulate(true);effects[name]=effect
   else report.errors[#report.errors+1]="missing effect "..name end
  end
  report.version=VERSIONSTR
  active=true
 end)
end)
hook.Add("PostRender","hp_weighted117",function()
 if not active then return end
 frame=frame+1
 if frame~=5 then return end
 active=false
 for name,effect in pairs(effects)do
  local low,high=effect:GetRenderBounds()
  local head=Vector(math.abs(low.x)>math.abs(high.x)and low.x or high.x,0,64+(math.abs(low.z)>math.abs(high.z)and low.z or high.z))
  local tone=render.GetToneMappingScaleLinear()
  render.SetToneMappingScaleLinear(Vector(1,1,1))
  render.PushRenderTarget(target);render.Clear(0,0,0,255,true,true)
  cam.Start3D(head+Vector(0,-256,0),Angle(0,90,0),60,0,0,128,128,1,2000)
  render.SuppressEngineLighting(true);render.ResetModelLighting(1,1,1);effect:Render();render.SuppressEngineLighting(false);cam.End3D();render.PopRenderTarget()
  render.PushRenderTarget(readback);render.Clear(0,0,0,255,true,true)
  cam.Start2D();render.SetMaterial(blit);render.DrawScreenQuad();render.CapturePixels()
  local count=0
  local rows={}
  for y=0,127 do
   local row={128,-1}
   for x=0,127 do local r,g,b=render.ReadPixel(x,y);if math.max(r,g,b)>4 then count=count+1;row[1]=math.min(row[1],x);row[2]=math.max(row[2],x)end end
   rows[#rows+1]=row
  end
  local png=render.Capture({format="png",x=0,y=0,w=128,h=128,alpha=false})
  if png then file.Write("hp_weighted117_"..name..".png",png)end
  report.cases[name]={count=count,rows=rows,head={head.x,head.y,head.z},eye={head.x,-256,head.z},low={low.x,low.y,low.z},high={high.x,high.y,high.z}}
  cam.End2D();render.PopRenderTarget();render.SetToneMappingScaleLinear(tone)
  effect:StopEmissionAndDestroyImmediately()
 end
 file.Write("hp_weighted117_reference.json",util.TableToJSON(report,true))
end)
