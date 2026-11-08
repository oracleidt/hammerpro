local prefix="hp_conv117_"
local identity="center .5 .5 scale 1 1 rotate 0 translate 0 0"
local turned="center .5 .5 scale 1 1 rotate 90 translate .25 0"
local report={cases={},errors={},version=tostring(VERSIONSTR),transforms={}}
local active=false
local target, lightmap, white, black, pattern, normals, flat
local function rt(name) return GetRenderTarget(prefix..name,128,128,false) end
local function paint(texture, draw)
 render.PushRenderTarget(texture);cam.Start2D();render.Clear(0,0,0,255,true,true);draw();cam.End2D();render.PopRenderTarget()
end
local function rectangle(x,y,w,h,r,g,b,a)
 surface.SetDrawColor(r,g,b,a or 255);surface.DrawRect(x,y,w,h)
end
local function initialize()
 assert(game.MountGMA("data/hp_conv117_assets.gma"),"own cube fixture mount failed")
 report.specular=GetConVar("mat_specular"):GetInt()
 report.hdr_level=GetConVar("mat_hdr_level"):GetInt()
 if render.GetToneMappingScaleLinear then report.tone=tostring(render.GetToneMappingScaleLinear())end
 target=rt("output");white=rt("white");black=rt("black");pattern=rt("pattern");normals=rt("normals");flat=rt("flat");lightmap=rt("lightmap")
 paint(white,function()rectangle(0,0,128,128,255,255,255)end)
 paint(black,function()rectangle(0,0,128,128,0,0,0)end)
 paint(flat,function()rectangle(0,0,128,128,128,128,255)end)
 paint(pattern,function()
  for y=0,3 do for x=0,3 do rectangle(x*32,y*32,32,32,x*80,y*80,(x*4+y)*17)end end
 end)
 paint(normals,function()
  for y=0,3 do for x=0,3 do
   local a,b=(x-1.5)*.35,(y-1.5)*.35
   rectangle(x*32,y*32,32,32,(a*.5+.5)*255,(b*.5+.5)*255,(math.sqrt(1-a*a-b*b)*.5+.5)*255)
  end end
 end)
 paint(lightmap,function()
  rectangle(0,0,32,128,255,255,255);rectangle(32,0,32,128,255,20,20);rectangle(64,0,32,128,20,255,20);rectangle(96,0,32,128,20,20,255)
 end)
end
local function draw(material, shader)
 local object=Mesh(material)
 local vertices={{-1,-1,0,0},{1,-1,1,0},{1,1,1,1},{-1,1,0,1}}
 mesh.Begin(object,MATERIAL_QUADS,1)
 for _,v in ipairs(vertices) do
  mesh.Position(Vector(0,v[1],v[2]));mesh.Normal(Vector(-1,0,0));mesh.Color(255,255,255,255);mesh.TexCoord(0,v[3],v[4])
  if shader=="LightmappedGeneric" or shader=="WorldVertexTransition" or shader=="Lightmapped_4WayBlend" or shader=="WorldTwoTextureBlend" then
   mesh.TexCoord(1,.125,.5);mesh.TexCoord(2,.25,0);mesh.TangentS(Vector(0,1,0));mesh.TangentT(Vector(0,0,1))
  elseif shader=="VertexLitGeneric" then mesh.UserData(0,1,0,-1) end
  mesh.AdvanceVertex()
 end
 mesh.End()
 render.PushRenderTarget(target);render.Clear(0,0,0,255,true,true)
 cam.Start3D(Vector(-4,0,0),Angle(0,0,0),40,0,0,128,128,1,100)
 render.SuppressEngineLighting(true);render.ResetModelLighting(.05,.05,.05);render.SetModelLighting(2,1,1,1)
 render.SetColorModulation(1,1,1);render.SetBlend(1);render.SetMaterial(material)
 if shader=="LightmappedGeneric" or shader=="WorldVertexTransition" or shader=="Lightmapped_4WayBlend" or shader=="WorldTwoTextureBlend" then render.SetLightmapTexture(lightmap)end
 object:Draw();render.SuppressEngineLighting(false);cam.End3D()
 cam.Start2D();render.CapturePixels()
 local pixels={}
 for y=32,95,8 do for x=32,95,8 do local r,g,b=render.ReadPixel(x,y);pixels[#pixels+1]={r,g,b}end end
 local png=render.Capture({format="png",x=0,y=0,w=128,h=128,alpha=false})
 cam.End2D();render.PopRenderTarget();object:Destroy()
 return pixels,png
end

hook.Add("InitPostEntity",prefix.."start",function()timer.Simple(.5,function()gui.HideGameUI();active=true end)end)
hook.Add("PostRender",prefix.."probe",function()
 if not active then return end;active=false
 local ok,error=xpcall(function()
  initialize();render.SetToneMappingScaleLinear(Vector(1,1,1))
  local cases={{"original","LightmappedGeneric","hp_conv117/reflection"},{"before","VertexLitGeneric","hp_conv117/before"},{"after","VertexLitGeneric","models/hp_conv117/probe/m_reflection_da8226c00826499d"}}
  for _,case in ipairs(cases)do
   local mat=Material(case[3]);assert(not mat:IsError(),case[3])
   local pixels,png=draw(mat,case[2])
   report.cases[#report.cases+1]={name=case[1],shader=mat:GetShader(),tint=tostring(mat:GetVector("$envmaptint")),pixels=pixels}
   assert(png,"no PNG");file.Write(prefix..case[1]..".png",png)
  end
 end,debug.traceback)
 if not ok then report.errors[#report.errors+1]=tostring(error)end
 file.Write("hp_conv117_reference.json",util.TableToJSON(report,true))
end)
