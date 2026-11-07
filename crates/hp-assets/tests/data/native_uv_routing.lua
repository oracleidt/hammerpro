local prefix="hp_uvrouting117_"
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
 assert(game.MountGMA("data/hp_uvrouting117_assets.gma"),"own cube fixture mount failed")
 report.specular=GetConVar("mat_specular"):GetInt()
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
   {"unlit_base","UnlitGeneric","$basetexturetransform",props({["$basetexture"]=pattern:GetName()})},
   {"unlit_detail","UnlitGeneric","$detailtexturetransform",props({["$detail"]=pattern:GetName(),["$detailblendmode"]="2",["$detailscale"]="1"})},
   {"unlit_detail_base","UnlitGeneric","$basetexturetransform",props({["$detail"]=pattern:GetName(),["$detailblendmode"]="2",["$detailscale"]="1"})},
   {"brush_bump","LightmappedGeneric","$bumptransform",props({["$bumpmap"]=normals:GetName()})},
   {"brush_bump_detail","LightmappedGeneric","$bumptransform",props({["$bumpmap"]=normals:GetName(),["$detail"]=white:GetName(),["$detailscale"]="1"})},
   {"brush_detail","LightmappedGeneric","$detailtexturetransform",props({["$detail"]=pattern:GetName(),["$detailblendmode"]="2",["$detailscale"]="1"})},
   {"blend_base2","WorldVertexTransition","$basetexturetransform2",props({["$basetexture2"]=pattern:GetName()})},
   {"blend_bump2","WorldVertexTransition","$bumptransform",props({["$basetexture2"]=white:GetName(),["$bumpmap"]=flat:GetName(),["$bumpmap2"]=normals:GetName()})},
   {"blend_bump2_detail","WorldVertexTransition","$bumptransform",props({["$basetexture2"]=white:GetName(),["$bumpmap"]=flat:GetName(),["$bumpmap2"]=normals:GetName(),["$detail"]=white:GetName(),["$detailscale"]="1"})},
   {"blend_bump2_own","WorldVertexTransition","$bumptransform2",props({["$basetexture2"]=white:GetName(),["$bumpmap"]=flat:GetName(),["$bumpmap2"]=normals:GetName()})},
   {"blend_bump2_fast","WorldVertexTransition","$bumptransform",props({["$basetexture2"]=white:GetName(),["$bumpmap"]=flat:GetName(),["$bumpmap2"]=normals:GetName(),["$blendmodulatetexture"]=white:GetName()})},
   {"brush_mask_bump1","LightmappedGeneric","$bumptransform",props({["$bumpmap"]=normals:GetName(),["$bumpmap2"]=flat:GetName(),["$bumpmask"]=flat:GetName()})},
   {"brush_mask_bump2","LightmappedGeneric","$bumptransform2",props({["$bumpmap"]=flat:GetName(),["$bumpmap2"]=normals:GetName(),["$bumpmask"]=flat:GetName()})},
   {"model_detail","VertexLitGeneric","$detailtexturetransform",props({["$detail"]=pattern:GetName(),["$detailblendmode"]="2",["$detailscale"]="1"})},
   {"model_detail_bump","VertexLitGeneric","$detailtexturetransform",props({["$bumpmap"]=flat:GetName(),["$detail"]=pattern:GetName(),["$detailblendmode"]="2",["$detailscale"]="1"})},
   {"four_bump1","Lightmapped_4WayBlend","$bumptransform",props({["$basetexture2"]=white:GetName(),["$basetexture3"]=white:GetName(),["$basetexture4"]=white:GetName(),["$bumpmap"]=normals:GetName()})},
   {"four_bump2","Lightmapped_4WayBlend","$bumptransform2",props({["$basetexture2"]=white:GetName(),["$basetexture3"]=white:GetName(),["$basetexture4"]=white:GetName(),["$bumpmap"]=flat:GetName(),["$bumpmap2"]=normals:GetName(),["$texture2_bumpblendfactor"]="1"})},
   {"four_base2","Lightmapped_4WayBlend","$basetexturetransform2",props({["$basetexture2"]=pattern:GetName(),["$basetexture3"]=white:GetName(),["$basetexture4"]=white:GetName()})},
   {"two_bump","WorldTwoTextureBlend","$bumptransform",props({["$bumpmap"]=normals:GetName(),["$detail"]=white:GetName(),["$detailscale"]="1"})},
   {"four_detail","Lightmapped_4WayBlend","$detailtexturetransform",props({["$basetexture2"]=white:GetName(),["$basetexture3"]=white:GetName(),["$basetexture4"]=white:GetName(),["$detail"]=pattern:GetName(),["$detailblendmode"]="0",["$detailscale"]="1"})},
   {"two_detail","WorldTwoTextureBlend","$detailtexturetransform",props({["$detail"]=pattern:GetName(),["$detailscale"]="1"})},
   {"model_bump","VertexLitGeneric","$bumptransform",props({["$bumpmap"]=normals:GetName()})},
   {"brush_envmask","LightmappedGeneric","$envmapmasktransform",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_uvrouting117/white_cube",["$envmapmask"]=pattern:GetName()})},
   {"model_envmask","VertexLitGeneric","$envmapmasktransform",props({["$basetexture"]=black:GetName(),["$envmap"]="hp_uvrouting117/white_cube",["$envmapmask"]=pattern:GetName()})},
  }
  for _,case in ipairs(cases)do
   local record={name=case[1],shader=case[2],parameter=case[3],properties=case[4],variants={}}
   for _,variant in ipairs({"identity","parameter","base"})do
    local p=table.Copy(case[4]);if variant=="parameter"then p[case[3]]=turned elseif variant=="base"then p["$basetexturetransform"]=turned end
    local material=CreateMaterial(prefix..case[1].."_"..variant,case[2],p)
    assert(not material:IsError(),case[1].." material failed")

    if variant=="identity" and string.find(case[1],"envmask")then
     local tex=material:GetTexture("$envmap");local tint=material:GetVector("$envmaptint")
     record.env={shader=material:GetShader(),texture=tex and tex:GetName()or"nil",width=tex and tex:Width()or 0,tint=tint and tostring(tint)or"nil",mounted=file.Exists("materials/hp_uvrouting117/white_cube.vtf","GAME")}
    end
    local pixels,png=draw(material,case[2]);record.variants[variant]=pixels
    if png then file.Write(prefix..case[1].."_"..variant..".png",png)end
   end
   report.cases[#report.cases+1]=record
  end
 end,debug.traceback)
 if not ok then report.errors[#report.errors+1]=tostring(error)end
 file.Write("hp_uv_routing117_reference.json",util.TableToJSON(report,true))
end)
