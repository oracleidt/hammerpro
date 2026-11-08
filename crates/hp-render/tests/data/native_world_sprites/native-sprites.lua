local cases={{["name"]="glow_d100",["mode"]=3,["distance"]=100},{["name"]="glow_d200",["mode"]=3,["distance"]=200},{["name"]="glow_d400",["mode"]=3,["distance"]=400},{["name"]="glow_d2400",["mode"]=3,["distance"]=2400},{["name"]="world_d200",["mode"]=9,["distance"]=200},{["name"]="world_d400",["mode"]=9,["distance"]=400},{["name"]="no_diss_d400",["mode"]=3,["fx"]=14,["distance"]=400},{["name"]="alpha_d200",["mode"]=2,["distance"]=200},{["name"]="add_d200",["mode"]=5,["distance"]=200},{["name"]="add_half_d200",["mode"]=5,["amount"]=128,["distance"]=200},{["name"]="frameblend_d200",["mode"]=7,["distance"]=200},{["name"]="plain_d200",["mode"]=0,["distance"]=200},{["name"]="legacy_d200",["mode"]=3,["legacy"]=true,["distance"]=200},{["name"]="roll_d200",["mode"]=3,["roll"]=30,["distance"]=200},{["name"]="scaled_d200",["mode"]=3,["scale"]=0.25,["distance"]=200},{["name"]="alphaadd_d200",["mode"]=8,["distance"]=200},{["name"]="hdr_d200",["mode"]=3,["hdr"]=0.7,["distance"]=200},{["name"]="legacy_hdr_d200",["mode"]=3,["hdr"]=0.7,["legacy"]=true,["distance"]=200},{["name"]="centered_d200",["mode"]=3,["material"]="centered",["origin"]={0.5,0.5},["distance"]=200},{["name"]="default_add_d200",["mode"]=5,["material"]="default",["distance"]=200},{["name"]="unlit_d200",["mode"]=3,["material"]="unlit",["distance"]=200},{["name"]="unlit_bg_d200",["mode"]=3,["material"]="unlit",["distance"]=200,["bg"]=true},{["name"]="unlit_add_bg_d200",["mode"]=3,["material"]="unlit_add",["distance"]=200,["bg"]=true},{["name"]="unlit_opaque_bg_d200",["mode"]=3,["material"]="unlit_opaque",["distance"]=200,["bg"]=true},{["name"]="unlit_plain_bg_d200",["mode"]=0,["material"]="unlit",["distance"]=200,["bg"]=true},{["name"]="unlit_novertex_bg_d200",["mode"]=3,["material"]="unlit_novertex",["distance"]=200,["bg"]=true},{["name"]="sprite_bg_d200",["mode"]=3,["distance"]=200,["bg"]=true}}
local result={cases={},errors={},version=VERSIONSTR}
local index=1;local active=false;local ids={};local frames=0
local target=GetRenderTargetEx("hp_sprite118_hdr",128,128,RT_SIZE_NO_CHANGE,MATERIAL_RT_DEPTH_SEPARATE,1,CREATERENDERTARGETFLAGS_HDR,IMAGE_FORMAT_RGBA16161616F)
local readback=GetRenderTarget("hp_sprite118_readback",128,128)
local blit=CreateMaterial("hp_sprite118_copy","UnlitGeneric",{["$basetexture"]=target:GetName(),["$ignorez"]="1",["$nofog"]="1"})
hook.Add("HUDShouldDraw","hp_sprite118",function()return false end)
hook.Add("CalcView","hp_sprite118",function()local c=cases[math.min(index,#cases)];if active then for _,n in pairs(ids)do local e=Entity(n);if IsValid(e)then e:SetNoDraw(n~=ids[c.name])end end end;return{origin=Vector(0,-c.distance,64),angles=Angle(0,90,0),fov=90,drawviewer=false}end)
hook.Add("InitPostEntity","hp_sprite118",function()gui.HideGameUI();timer.Simple(1,function()
 ids=util.JSONToTable(file.Read("hp_sprite118_entities.json","DATA")or"{}")or{}
 for _,n in pairs(ids)do local e=Entity(n);if IsValid(e)then e:SetNoDraw(true)end end
 active=true
end)end)
hook.Add("PostRender","hp_sprite118",function()
 if not active then return end
 frames=frames+1;if frames<4 then return end;frames=0
 local case=cases[index];local e=Entity(ids[case.name]or-1)
 if not IsValid(e)then result.errors[#result.errors+1]="missing "..case.name else
  local tone=render.GetToneMappingScaleLinear();render.SetToneMappingScaleLinear(Vector(1,1,1))
  render.PushRenderTarget(target);if case.bg then render.Clear(32,64,128,255,true,true)else render.Clear(0,0,0,255,true,true)end
  cam.Start3D(Vector(0,-case.distance,64),Angle(0,90,0),90,0,0,128,128,1,5000)
  e:SetNoDraw(false);e:DrawModel();e:SetNoDraw(true);cam.End3D();render.PopRenderTarget()
  render.PushRenderTarget(readback);render.Clear(0,0,0,255,true,true);cam.Start2D();render.SetMaterial(blit);render.DrawScreenQuad();render.CapturePixels()
  local count=0;local lo={128,128};local hi={-1,-1};local rows={};local maxrgb={0,0,0}
  for y=0,127 do local row={128,-1};for x=0,127 do local r,g,b=render.ReadPixel(x,y);if math.max(r,g,b)>3 then count=count+1;row[1]=math.min(row[1],x);row[2]=math.max(row[2],x);lo[1]=math.min(lo[1],x);lo[2]=math.min(lo[2],y);hi[1]=math.max(hi[1],x);hi[2]=math.max(hi[2],y);maxrgb[1]=math.max(maxrgb[1],r);maxrgb[2]=math.max(maxrgb[2],g);maxrgb[3]=math.max(maxrgb[3],b)end end;rows[#rows+1]=row end
  local r,g,b=render.ReadPixel(64,64)
  file.Write("hp_sprite118_"..case.name..".png",render.Capture({format="png",x=0,y=0,w=128,h=128,alpha=false}))
  result.cases[case.name]={count=count,low=lo,high=hi,rows=rows,center={r,g,b},maximum=maxrgb,mode=e:GetRenderMode(),model=e:GetModel(),scale=e:GetInternalVariable("m_flSpriteScale")}
  cam.End2D();render.PopRenderTarget();render.SetToneMappingScaleLinear(tone)
 end
 index=index+1;if index>#cases then active=false;file.Write("hp_sprite118_reference.json",util.TableToJSON(result,true))end
end)
