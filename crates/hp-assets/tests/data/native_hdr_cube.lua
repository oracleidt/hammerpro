local prefix="hp_hdrcube117_"
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
 assert(game.MountGMA("data/hp_hdrcube117_assets.gma"),"own cube fixture mount failed")
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
 if not active then return end
 active=false
 local ok,error=xpcall(function()
  initialize()
  local matrixmaterial=CreateMaterial(prefix.."matrix","UnlitGeneric",{["$basetexture"]=white:GetName(),["$basetexturetransform"]=turned})
  local matrix=matrixmaterial:GetMatrix("$basetexturetransform")
  for row=1,2 do report.transforms[row]={}for _,col in ipairs({1,2,4})do table.insert(report.transforms[row],string.format("%.17g",matrix:GetField(row,col)))end end
  local function props(values)
   local p={["$basetexture"]=white:GetName(),["$basetexturetransform"]=identity,["$nocull"]="1"}
   for k,v in pairs(values or{})do p[k]=v end
   return p
  end
  local cases={
   {"LightmappedGeneric_half_1","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]"})},
   {"LightmappedGeneric_half_025","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.025 .025 .025]"})},
   {"LightmappedGeneric_wide_1","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.1 .1 .1]"})},
   {"LightmappedGeneric_wide_025","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.025 .025 .025]"})},
   {"LightmappedGeneric_ldr_1","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.1 .1 .1]"})},
   {"LightmappedGeneric_ldr_025","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.025 .025 .025]"})},
   {"VertexLitGeneric_half_1","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]"})},
   {"VertexLitGeneric_half_025","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.025 .025 .025]"})},
   {"VertexLitGeneric_wide_1","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.1 .1 .1]"})},
   {"VertexLitGeneric_wide_025","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.025 .025 .025]"})},
   {"VertexLitGeneric_ldr_1","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.1 .1 .1]"})},
   {"VertexLitGeneric_ldr_025","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.025 .025 .025]"})},
   {"UnlitGeneric_half_1","UnlitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]"})},
   {"UnlitGeneric_half_025","UnlitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.025 .025 .025]"})},
   {"UnlitGeneric_wide_1","UnlitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.1 .1 .1]"})},
   {"UnlitGeneric_wide_025","UnlitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.025 .025 .025]"})},
   {"UnlitGeneric_ldr_1","UnlitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.1 .1 .1]"})},
   {"UnlitGeneric_ldr_025","UnlitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.025 .025 .025]"})},
   {"WorldVertexTransition_half_1","WorldVertexTransition","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]",["$basetexture2"]=black:GetName()})},
   {"WorldVertexTransition_half_025","WorldVertexTransition","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.025 .025 .025]",["$basetexture2"]=black:GetName()})},
   {"WorldVertexTransition_wide_1","WorldVertexTransition","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.1 .1 .1]",["$basetexture2"]=black:GetName()})},
   {"WorldVertexTransition_wide_025","WorldVertexTransition","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.025 .025 .025]",["$basetexture2"]=black:GetName()})},
   {"WorldVertexTransition_ldr_1","WorldVertexTransition","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.1 .1 .1]",["$basetexture2"]=black:GetName()})},
   {"WorldVertexTransition_ldr_025","WorldVertexTransition","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.025 .025 .025]",["$basetexture2"]=black:GetName()})},
   {"Lightmapped_4WayBlend_half_1","Lightmapped_4WayBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]",["$basetexture2"]=black:GetName(),["$basetexture3"]=black:GetName(),["$basetexture4"]=black:GetName()})},
   {"Lightmapped_4WayBlend_half_025","Lightmapped_4WayBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.025 .025 .025]",["$basetexture2"]=black:GetName(),["$basetexture3"]=black:GetName(),["$basetexture4"]=black:GetName()})},
   {"Lightmapped_4WayBlend_wide_1","Lightmapped_4WayBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.1 .1 .1]",["$basetexture2"]=black:GetName(),["$basetexture3"]=black:GetName(),["$basetexture4"]=black:GetName()})},
   {"Lightmapped_4WayBlend_wide_025","Lightmapped_4WayBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.025 .025 .025]",["$basetexture2"]=black:GetName(),["$basetexture3"]=black:GetName(),["$basetexture4"]=black:GetName()})},
   {"Lightmapped_4WayBlend_ldr_1","Lightmapped_4WayBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.1 .1 .1]",["$basetexture2"]=black:GetName(),["$basetexture3"]=black:GetName(),["$basetexture4"]=black:GetName()})},
   {"Lightmapped_4WayBlend_ldr_025","Lightmapped_4WayBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.025 .025 .025]",["$basetexture2"]=black:GetName(),["$basetexture3"]=black:GetName(),["$basetexture4"]=black:GetName()})},
   {"WorldTwoTextureBlend_half_1","WorldTwoTextureBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$detail"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]"})},
   {"WorldTwoTextureBlend_half_025","WorldTwoTextureBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$detail"]=black:GetName(),["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.025 .025 .025]"})},
   {"WorldTwoTextureBlend_wide_1","WorldTwoTextureBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$detail"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.1 .1 .1]"})},
   {"WorldTwoTextureBlend_wide_025","WorldTwoTextureBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$detail"]=black:GetName(),["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.025 .025 .025]"})},
   {"WorldTwoTextureBlend_ldr_1","WorldTwoTextureBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$detail"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.1 .1 .1]"})},
   {"WorldTwoTextureBlend_ldr_025","WorldTwoTextureBlend","$envmaptint",props({["$basetexture"]=black:GetName(),["$detail"]=black:GetName(),["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.025 .025 .025]"})},
   {"Phong_half_1","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$bumpmap"]=flat:GetName(),["$phong"]="1",["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]"})},
   {"Phong_half_025","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$bumpmap"]=flat:GetName(),["$phong"]="1",["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.025 .025 .025]"})},
   {"Phong_wide_1","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$bumpmap"]=flat:GetName(),["$phong"]="1",["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.1 .1 .1]"})},
   {"Phong_wide_025","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$bumpmap"]=flat:GetName(),["$phong"]="1",["$envmap"]="hp_hdrcube117/wide",["$envmaptint"]="[.025 .025 .025]"})},
   {"Phong_ldr_1","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$bumpmap"]=flat:GetName(),["$phong"]="1",["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.1 .1 .1]"})},
   {"Phong_ldr_025","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$bumpmap"]=flat:GetName(),["$phong"]="1",["$envmap"]="hp_hdrcube117/ldr",["$envmaptint"]="[.025 .025 .025]"})},
   {"LightmappedGeneric_dim_01","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/dim",["$envmaptint"]="[.01 .01 .01]"})},
   {"LightmappedGeneric_dim_5","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/dim",["$envmaptint"]="[.5 .5 .5]"})},
   {"LightmappedGeneric_dim_2","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/dim",["$envmaptint"]="[2 2 2]"})},
   {"LightmappedGeneric_dim_4","LightmappedGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/dim",["$envmaptint"]="[4 4 4]"})},
   {"VertexLitGeneric_dim_01","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/dim",["$envmaptint"]="[.01 .01 .01]"})},
   {"VertexLitGeneric_dim_5","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/dim",["$envmaptint"]="[.5 .5 .5]"})},
   {"VertexLitGeneric_dim_2","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/dim",["$envmaptint"]="[2 2 2]"})},
   {"VertexLitGeneric_dim_4","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_hdrcube117/dim",["$envmaptint"]="[4 4 4]"})},
   {"PhongNoNormal","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$phong"]="1",["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]"})},
   {"PhongBaseMask","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$phong"]="1",["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]",["$basemapalphaphongmask"]="1"})},
   {"PhongWarp","VertexLitGeneric","$envmaptint",props({["$basetexture"]=black:GetName(),["$phong"]="1",["$envmap"]="hp_hdrcube117/half",["$envmaptint"]="[.1 .1 .1]",["$diffusewarptexture"]=white:GetName()})},
  }
  for _,case in ipairs(cases)do
   local record={name=case[1],shader=case[2],parameter=case[3],properties=case[4],variants={}}
   for _,variant in ipairs({"identity"})do
    local p=table.Copy(case[4]);if variant=="parameter"then p[case[3]]=turned elseif variant=="base"then p["$basetexturetransform"]=turned end
    local material=CreateMaterial(prefix..case[1].."_"..variant,case[2],p)
    assert(not material:IsError(),case[1].." material failed")

    if variant=="identity" then
     local tex=material:GetTexture("$envmap");local tint=material:GetVector("$envmaptint")
     record.env={shader=material:GetShader(),texture=tex and tex:GetName()or"nil",width=tex and tex:Width()or 0,tint=tint and tostring(tint)or"nil",mounted=file.Exists("materials/"..p["$envmap"]..".vtf","GAME")}
    end
    local pixels,png=draw(material,case[2]);record.variants[variant]=pixels
    if png then file.Write(prefix..case[1].."_"..variant..".png",png)end
   end
   report.cases[#report.cases+1]=record
  end
 end,debug.traceback)
 if not ok then report.errors[#report.errors+1]=tostring(error)end
 file.Write("hp_hdr_cube117_reference.json",util.TableToJSON(report,true))
end)
