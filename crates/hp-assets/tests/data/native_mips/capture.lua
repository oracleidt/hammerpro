local result={cases={},errors={}}
local target=GetRenderTarget("hp_render118_mips",128,128)
local ready=false
result.cvars={}
for _,name in ipairs({"mat_picmip","mat_mipmaptextures","mat_trilinear","mat_forceaniso","mat_filtertextures","mat_reducefillrate","mat_texture_limit","r_mipmappinglod"})do local cv=GetConVar(name);if cv then result.cvars[name]=cv:GetString()end end
hook.Add("HUDShouldDraw","hp_render118_mips",function()return false end)
hook.Add("InitPostEntity","hp_render118_mips",function()
 gui.HideGameUI()
 timer.Simple(1,function()
  result.mounted=game.MountGMA("data/hp_render118.gma")
  if not result.mounted then result.errors[#result.errors+1]="GMA missing"end
  ready=true
 end)
end)
hook.Add("PostRender","hp_render118_mips",function()
 if not ready then return end
 ready=false
 for _,name in ipairs({"tail_rgb","tail_rgb_wide","levels_bc","levels_1024","levels_all","levels_nomip","levels_wide"})do
  local mat=CreateMaterial("hp_render118_"..name,"UnlitGeneric",{["$basetexture"]="hp_render118/"..name,["$ignorez"]="1",["$nocull"]="1",["$vertexcolor"]="1",["$vertexalpha"]="1"})
  for _,repeatuv in ipairs({1,2,4,8,16,32,64,128,256,512})do
   render.PushRenderTarget(target);render.Clear(0,0,0,255,true,true);cam.Start3D(Vector(0,-128,0),Angle(0,90,0),90,0,0,128,128,1,1000)
   render.SetMaterial(mat);mesh.Begin(MATERIAL_QUADS,1)
   for _,v in ipairs({{-128,0,-128,0,repeatuv},{-128,0,128,0,0},{128,0,128,repeatuv,0},{128,0,-128,repeatuv,repeatuv}})do mesh.Position(Vector(v[1],v[2],v[3]));mesh.TexCoord(0,v[4],v[5]);mesh.Color(255,255,255,255);mesh.AdvanceVertex()end
   mesh.End()
   render.CapturePixels();local sum={0,0,0};local low={255,255,255};local high={0,0,0}
   for y=0,127 do for x=0,127 do local r,g,b=render.ReadPixel(x,y);for k,v in ipairs({r,g,b})do sum[k]=sum[k]+v;low[k]=math.min(low[k],v);high[k]=math.max(high[k],v)end end end
   for k=1,3 do sum[k]=sum[k]/16384 end
   local key=name.."_"..repeatuv;result.cases[key]={mean=sum,low=low,high=high,width=mat:GetTexture("$basetexture"):Width()}
   file.Write("hp_render118_"..key..".png",render.Capture({format="png",x=0,y=0,w=128,h=128,alpha=false}))
   cam.End3D();render.PopRenderTarget()
  end
 end
 file.Write("hp_render118_mips.json",util.TableToJSON(result,true))
end)
