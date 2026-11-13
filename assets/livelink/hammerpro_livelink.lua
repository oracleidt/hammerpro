if HAMMERPRO_LIVELINK and HAMMERPRO_LIVELINK.v == 1 then return end
HAMMERPRO_LIVELINK = { v = 1 }
local LL = HAMMERPRO_LIVELINK

local DIR = "hammerpro_livelink"
local NET = "hammerpro_livelink"
local INJECT = "RunString(file.Read('hammerpro_livelink/addon.txt','DATA') or '','hammerpro_livelink')"
local BLOCKED = { point_servercommand = true, point_clientcommand = true, point_broadcastclientcommand = true, lua_run = true }
local BLOCKED_INPUT = { command = true, runcode = true, runpassedcode = true }

local src = debug.getinfo(1, "S")
src = src and src.source or ""
LL.mode = string.find(src, "autorun", 1, true) and "mounted" or "injected"

local function read_json(path)
	local text = file.Read(path, "DATA")
	if not text or #text == 0 or #text > 8388608 then return nil, text end
	local ok, value = pcall(util.JSONToTable, text)
	if ok and istable(value) then return value, text end
	return nil, text
end

local function nums3(t, limit)
	if not istable(t) then return nil end
	local a, b, c = tonumber(t[1]), tonumber(t[2]), tonumber(t[3])
	if not (a and b and c) then return nil end
	limit = limit or 131072
	if math.abs(a) > limit or math.abs(b) > limit or math.abs(c) > limit then return nil end
	return a, b, c
end

local function words3(s)
	if not isstring(s) then return nil end
	local x, y, z = string.match(s, "^%s*(%S+)%s+(%S+)%s+(%S+)")
	x, y, z = tonumber(x), tonumber(y), tonumber(z)
	if x and y and z then return x, y, z end
	return nil
end

if SERVER then
	if LL.mode == "mounted" then pcall(AddCSLuaFile) end
	util.AddNetworkString(NET)
	file.CreateDir(DIR)

	local hammerids = setmetatable({}, { __mode = "k" })
	hook.Add("EntityKeyValue", "hammerpro_livelink", function(ent, key, value)
		if key == "hammerid" then hammerids[ent] = tonumber(value) end
	end)

	local load_id = game.GetMap() .. ":" .. os.time() .. ":" .. math.random(100000, 999999)
	local token = nil
	local labels = {}
	local follow = false
	local last_seq = 0
	local results = {}
	local created = {}
	local ghosts = {}
	local lights = {}
	local back_state = nil
	local ents_rev = 0
	local last_text = nil
	local report_due = true
	local next_beat = 0
	local last_eye = nil

	local function host()
		if game.SinglePlayer() then return Entity(1) end
		for _, p in ipairs(player.GetHumans()) do
			if p:IsListenServerHost() then return p end
		end
		return nil
	end

	local function count(t)
		local n = 0
		for _ in pairs(t) do n = n + 1 end
		return n
	end

	local function to_clients(op, ply)
		local text = util.TableToJSON(op)
		if not text or #text > 60000 then return end
		net.Start(NET)
		net.WriteString(text)
		if ply then net.Send(ply) else net.Broadcast() end
	end

	local function write_report()
		local data = {
			v = 1,
			token = token,
			ack = last_seq,
			load = load_id,
			map = game.GetMap(),
			beat = os.time(),
			addon = LL.mode,
			results = results,
			ents_rev = ents_rev,
			stats = { created = count(created), ghosts = count(ghosts), lights = count(lights) },
		}
		local ply = host()
		if follow and IsValid(ply) then
			local e, a = ply:EyePos(), ply:EyeAngles()
			data.eye = { e.x, e.y, e.z }
			data.ang = { a.p, a.y, a.r }
			last_eye = { e, a }
		end
		file.Write(DIR .. "/game.json", util.TableToJSON(data) or "{}")
		report_due = false
		next_beat = SysTime() + 1
	end

	local function write_ents()
		ents_rev = ents_rev + 1
		local list = {}
		for _, e in ipairs(ents.GetAll()) do
			if IsValid(e) and e:CreatedByMap() then
				local id = e:MapCreationID()
				if id and id > 0 then
					local p = e:GetPos()
					list[#list + 1] = { i = id, c = e:GetClass(), n = e:GetName() or "", o = { p.x, p.y, p.z }, m = e:GetModel() or "", h = hammerids[e] or 0 }
					if #list >= 16384 then break end
				end
			end
		end
		file.Write(DIR .. "/ents.json", util.TableToJSON({ token = token, load = load_id, rev = ents_rev, ents = list }) or "{}")
	end

	local function result(seq, ok, msg)
		if ok then return end
		results[#results + 1] = { seq = seq, ok = false, msg = tostring(msg or "failed") }
		while #results > 20 do table.remove(results, 1) end
	end

	local function find_target(t)
		if not istable(t) then return nil end
		local e = nil
		if t.i then
			e = ents.GetMapCreatedEntity(tonumber(t.i) or -1)
		elseif t.own then
			e = created[tonumber(t.own) or -1]
		end
		if IsValid(e) and not BLOCKED[e:GetClass()] then return e end
		return nil
	end

	local function io_allowed(value)
		local parts = string.Explode(string.find(value, "\27", 1, true) and "\27" or ",", value)
		local input = string.lower(string.Trim(parts[2] or ""))
		local target = string.lower(string.Trim(parts[1] or ""))
		if BLOCKED_INPUT[input] then return false end
		for _, e in ipairs(ents.FindByName(target)) do
			if BLOCKED[e:GetClass()] then return false end
		end
		return true
	end

	local function apply_kv(ent, kv)
		local moved, remodel = false, false
		for k, v in pairs(kv) do
			k = string.lower(tostring(k))
			v = tostring(v)
			if k == "origin" then
				local x, y, z = words3(v)
				if x then ent:SetPos(Vector(x, y, z)) moved = true end
			elseif k == "angles" then
				local p, y, r = words3(v)
				if p then ent:SetAngles(Angle(p, y, r)) moved = true end
			elseif k == "model" then
				if string.match(string.lower(v), "%.mdl$") then
					util.PrecacheModel(v)
					ent:SetModel(v)
					remodel = true
				end
			elseif k == "skin" then
				ent:SetSkin(tonumber(v) or 0)
			elseif k == "rendercolor" then
				local r, g, b = words3(v)
				if r then
					local c = ent:GetColor()
					ent:SetColor(Color(r, g, b, c.a))
				end
			elseif k == "renderamt" then
				local c = ent:GetColor()
				ent:SetColor(Color(c.r, c.g, c.b, tonumber(v) or 255))
			elseif k == "rendermode" then
				ent:SetRenderMode(tonumber(v) or 0)
			elseif k == "renderfx" then
				ent:SetRenderFX(tonumber(v) or 0)
			elseif k == "modelscale" then
				ent:SetModelScale(tonumber(v) or 1, 0)
			elseif k == "targetname" then
				ent:SetName(v)
			elseif k == "disableshadows" then
				ent:DrawShadow(tonumber(v) ~= 1)
			else
				ent:SetKeyValue(k, v)
			end
		end
		local phys = ent:GetPhysicsObject()
		if remodel and string.find(ent:GetClass(), "prop_physics", 1, true) then
			ent:PhysicsInit(SOLID_VPHYSICS)
			phys = ent:GetPhysicsObject()
		end
		if (moved or remodel) and IsValid(phys) then
			phys:SetPos(ent:GetPos())
			phys:SetAngles(ent:GetAngles())
			phys:SetVelocity(vector_origin)
			phys:Wake()
		end
	end

	local function teleport(op)
		local ply = host()
		if not IsValid(ply) then return false, "no player" end
		local x, y, z = nums3(op.origin)
		local p, yw, r = nums3(op.angles, 360000)
		if not (x and p) then return false, "bad position" end
		if not back_state then
			back_state = { pos = ply:GetPos(), ang = ply:EyeAngles(), mt = ply:GetMoveType() }
		end
		if op.noclip then ply:SetMoveType(MOVETYPE_NOCLIP) end
		ply:SetPos(Vector(x, y, z) - ply:GetViewOffset())
		ply:SetEyeAngles(Angle(p, yw, 0))
		return true
	end

	local function back()
		local ply = host()
		if not IsValid(ply) then return false, "no player" end
		if not back_state then return false, "nothing to go back to" end
		ply:SetMoveType(back_state.mt)
		ply:SetPos(back_state.pos)
		ply:SetEyeAngles(back_state.ang)
		back_state = nil
		return true
	end

	local function fire(op)
		local input = tostring(op.input or "")
		if not string.match(input, "^[%w_]+$") or BLOCKED_INPUT[string.lower(input)] then
			return false, "input not allowed: " .. input
		end
		local targets = {}
		if op.target then
			local e = find_target(op.target)
			if e then targets[1] = e end
		elseif isstring(op.name) then
			if op.name == "!player" then
				targets[1] = host()
			else
				targets = ents.FindByName(op.name)
			end
		end
		local n = 0
		for _, e in ipairs(targets) do
			if IsValid(e) and not BLOCKED[e:GetClass()] then
				e:Fire(input, tostring(op.param or ""), tonumber(op.delay) or 0)
				n = n + 1
			end
		end
		if n == 0 then return false, "no entity named " .. tostring(op.name or "?") end
		return true
	end

	local function apply(op)
		local kind = op.op
		if kind == "set" then
			local e = find_target(op.target)
			if not e then return false, "entity not found in game" end
			if istable(op.kv) then apply_kv(e, op.kv) end
			return true
		elseif kind == "io" then
			local e = find_target(op.target)
			if not e then return false, "entity not found in game" end
			for _, c in ipairs(op.add or {}) do
				if istable(c) and isstring(c[1]) and isstring(c[2]) and io_allowed(c[2]) then e:SetKeyValue(c[1], c[2]) end
			end
			return true
		elseif kind == "create" then
			local cls = string.lower(tostring(op.class or ""))
			if BLOCKED[cls] then return false, "class not allowed: " .. cls end
			local key = tonumber(op.key) or -1
			if IsValid(created[key]) then created[key]:Remove() end
			local e = ents.Create(cls)
			if not IsValid(e) then return false, "cannot create " .. cls end
			for k, v in pairs(op.kv or {}) do e:SetKeyValue(tostring(k), tostring(v)) end
			for _, c in ipairs(op.io or {}) do
				if istable(c) and isstring(c[1]) and isstring(c[2]) and io_allowed(c[2]) then e:SetKeyValue(c[1], c[2]) end
			end
			e:Spawn()
			e:Activate()
			created[key] = e
			return true
		elseif kind == "remove" then
			local e = find_target(op.target)
			if not e then return false, "entity not found in game" end
			if op.target.own then created[tonumber(op.target.own) or -1] = nil end
			e:Remove()
			return true
		elseif kind == "ghost" then
			local key = tonumber(op.key) or -1
			ghosts[key] = (op.ghost or op.removed_at) and true or nil
			to_clients(op)
			return true
		elseif kind == "light" then
			local key = tonumber(op.key) or -1
			lights[key] = op.light and true or nil
			to_clients(op)
			return true
		elseif kind == "highlight" then
			local items = {}
			for _, it in ipairs(op.items or {}) do
				if istable(it) then
					local row = { mins = it.mins, maxs = it.maxs, ghost = it.ghost }
					local e = it.target and find_target(it.target)
					if IsValid(e) and e:EntIndex() > 0 then row.ent = e:EntIndex() end
					items[#items + 1] = row
					if #items >= 256 then break end
				end
			end
			to_clients({ op = "highlight", items = items })
			return true
		elseif kind == "fire" then
			return fire(op)
		elseif kind == "teleport" then
			return teleport(op)
		elseif kind == "back" then
			return back()
		elseif kind == "reset" then
			ghosts, lights = {}, {}
			to_clients({ op = "reset" })
			return true
		elseif kind == "entities" then
			write_ents()
			return true
		end
		return false, "unknown operation " .. tostring(kind)
	end

	local function state_to(ply)
		to_clients({ op = "state", active = token ~= nil, labels = labels }, ply)
	end

	local function stop_session()
		token = nil
		follow = false
		ghosts, lights = {}, {}
		last_text = nil
		state_to()
	end

	local function refresh_session()
		local s = read_json(DIR .. "/session.json")
		local valid = s and tonumber(s.v) == 1 and isstring(s.token) and #s.token >= 16 and math.abs(os.time() - (tonumber(s.beat) or 0)) <= 10
		if not valid then
			if token then stop_session() end
			return
		end
		labels = istable(s.labels) and s.labels or {}
		local was_follow = follow
		follow = s.follow == true
		if follow ~= was_follow then report_due = true end
		if s.token ~= token then
			token = s.token
			last_seq = 0
			results = {}
			last_text = nil
			if LL.mode == "injected" then BroadcastLua(INJECT) end
			write_report()
			write_ents()
			report_due = true
			timer.Simple(0.5, function() state_to() end)
		end
	end

	local function process()
		local cmd, text = read_json(DIR .. "/cmd.json")
		if text == last_text then return end
		last_text = text
		if not cmd or cmd.token ~= token or cmd.load ~= load_id then return end
		local done = 0
		for _, op in ipairs(cmd.ops or {}) do
			local seq = tonumber(op.seq) or 0
			if seq > last_seq then
				local ok, res, msg = pcall(apply, op)
				if not ok then
					result(seq, false, res)
				elseif res == false then
					result(seq, false, msg)
				end
				last_seq = seq
				report_due = true
				done = done + 1
				if done >= 256 then
					last_text = nil
					break
				end
			end
		end
	end

	local next_poll, next_session = 0, 0
	hook.Add("Think", "hammerpro_livelink", function()
		local now = SysTime()
		if now < next_poll then return end
		next_poll = now + 0.03
		if now >= next_session then
			next_session = now + 0.5
			refresh_session()
		end
		if not token then return end
		process()
		if follow and not report_due then
			local ply = host()
			if IsValid(ply) and last_eye then
				local e, a = ply:EyePos(), ply:EyeAngles()
				if e:DistToSqr(last_eye[1]) > 0.25 or math.abs(math.AngleDifference(a.y, last_eye[2].y)) > 0.2 or math.abs(a.p - last_eye[2].p) > 0.2 then
					report_due = true
				end
			elseif IsValid(ply) then
				report_due = true
			end
		end
		if report_due or now >= next_beat then write_report() end
	end)

	hook.Add("InitPostEntity", "hammerpro_livelink", function()
		if token then
			write_report()
			write_ents()
		end
	end)

	hook.Add("PlayerInitialSpawn", "hammerpro_livelink", function(ply)
		timer.Simple(2, function()
			if not IsValid(ply) then return end
			if LL.mode == "injected" then ply:SendLua(INJECT) end
			timer.Simple(0.5, function()
				if IsValid(ply) and token then state_to(ply) end
			end)
		end)
	end)
end

if CLIENT then
	local active = false
	local labels = {}
	local ghosts = {}
	local removed = {}
	local lights = {}
	local textures = {}
	local highlight = {}

	local function label(key, default)
		local v = labels[key]
		if isstring(v) and #v > 0 then return v end
		return default
	end

	local function clear_ghost(key)
		if IsValid(ghosts[key]) then ghosts[key]:Remove() end
		ghosts[key] = nil
		removed[key] = nil
	end

	local function clear_light(key)
		if IsValid(textures[key]) then textures[key]:Remove() end
		textures[key] = nil
		lights[key] = nil
	end

	local function clear_all()
		for k in pairs(ghosts) do clear_ghost(k) end
		for k in pairs(textures) do clear_light(k) end
		ghosts, removed, lights, textures, highlight = {}, {}, {}, {}, {}
	end

	local function set_ghost(key, g)
		clear_ghost(key)
		if not istable(g) then return end
		local model = tostring(g.model or "")
		if not string.match(string.lower(model), "%.mdl$") then return end
		local x, y, z = nums3(g.origin)
		local p, yw, r = nums3(g.angles, 360000)
		if not (x and p) then return end
		util.PrecacheModel(model)
		local cs = ClientsideModel(model, RENDERGROUP_BOTH)
		if not IsValid(cs) then return end
		cs:SetPos(Vector(x, y, z))
		cs:SetAngles(Angle(p, yw, r))
		cs:SetSkin(tonumber(g.skin) or 0)
		local c = istable(g.color) and g.color or {}
		local alpha = tonumber(c[4]) or 255
		if alpha < 255 then cs:SetRenderMode(RENDERMODE_TRANSCOLOR) end
		cs:SetColor(Color(tonumber(c[1]) or 255, tonumber(c[2]) or 255, tonumber(c[3]) or 255, alpha))
		local scale = tonumber(g.scale) or 1
		if scale ~= 1 then cs:SetModelScale(scale, 0) end
		ghosts[key] = cs
	end

	local function set_light(key, l)
		clear_light(key)
		if not istable(l) then return end
		local x, y, z = nums3(l.origin)
		local p, yw, r = nums3(l.angles, 360000)
		local cr, cg, cb = nums3(l.color, 255)
		if not (x and p and cr) then return end
		local light = {
			kind = l.kind == "spot" and "spot" or "point",
			pos = Vector(x, y, z),
			ang = Angle(p, yw, r),
			color = Color(cr, cg, cb),
			brightness = math.Clamp(tonumber(l.brightness) or 200, 0, 100000),
			outer = math.Clamp(tonumber(l.outer) or 45, 1, 89),
			distance = math.Clamp(tonumber(l.distance) or 512, 16, 8192),
		}
		lights[key] = light
		if light.kind == "spot" then
			local pt = ProjectedTexture()
			pt:SetTexture("effects/flashlight001")
			pt:SetPos(light.pos)
			pt:SetAngles(light.ang)
			pt:SetFOV(light.outer * 2)
			pt:SetNearZ(4)
			pt:SetFarZ(light.distance)
			pt:SetColor(light.color)
			pt:SetBrightness(math.Clamp(light.brightness / 150, 0.05, 8))
			pt:SetEnableShadows(true)
			pt:Update()
			textures[key] = pt
		end
	end

	local function handle(op)
		local kind = op.op
		local key = tonumber(op.key) or -1
		if kind == "state" then
			active = op.active == true
			labels = istable(op.labels) and op.labels or {}
			if not active then clear_all() end
		elseif kind == "ghost" then
			set_ghost(key, op.ghost)
			local x, y, z = nums3(op.removed_at)
			if x then removed[key] = Vector(x, y, z) end
		elseif kind == "light" then
			set_light(key, op.light)
		elseif kind == "highlight" then
			highlight = istable(op.items) and op.items or {}
		elseif kind == "reset" then
			clear_all()
		end
	end

	net.Receive(NET, function()
		local text = net.ReadString()
		local ok, op = pcall(util.JSONToTable, text or "")
		if ok and istable(op) then handle(op) end
	end)

	hook.Add("Think", "hammerpro_livelink", function()
		local now = CurTime()
		for key, l in pairs(lights) do
			if l.kind == "point" then
				local d = DynamicLight(60000 + (key % 4000))
				if d then
					d.pos = l.pos
					d.r, d.g, d.b = l.color.r, l.color.g, l.color.b
					d.brightness = math.Clamp(l.brightness / 300, 0.1, 3)
					d.Size = l.distance
					d.Decay = 0
					d.DieTime = now + 0.5
				end
			end
		end
	end)

	hook.Add("PreDrawHalos", "hammerpro_livelink", function()
		if #highlight == 0 then return end
		local list = {}
		for _, it in ipairs(highlight) do
			local e = it.ent and Entity(tonumber(it.ent) or -1)
			if IsValid(e) then list[#list + 1] = e end
			local g = it.ghost and ghosts[tonumber(it.ghost) or -1]
			if IsValid(g) then list[#list + 1] = g end
		end
		if #list > 0 then halo.Add(list, Color(255, 170, 0), 2, 2, 2, true, true) end
	end)

	local function text_at(pos, text, color)
		local ang = EyeAngles()
		ang:RotateAroundAxis(ang:Up(), -90)
		ang:RotateAroundAxis(ang:Forward(), 90)
		cam.Start3D2D(pos, ang, 0.12)
		draw.SimpleTextOutlined(text, "DermaLarge", 0, 0, color, TEXT_ALIGN_CENTER, TEXT_ALIGN_BOTTOM, 2, Color(0, 0, 0, 220))
		cam.End3D2D()
	end

	hook.Add("PostDrawTranslucentRenderables", "hammerpro_livelink", function(depth, sky)
		if sky or not active then return end
		local eye = EyePos()
		for _, it in ipairs(highlight) do
			local x, y, z = nums3(it.mins)
			local X, Y, Z = nums3(it.maxs)
			if x and X then
				render.DrawWireframeBox(vector_origin, angle_zero, Vector(x, y, z), Vector(X, Y, Z), Color(255, 170, 0), false)
			end
		end
		local preview = label("preview", "Hammer Pro preview")
		for _, l in pairs(lights) do
			if l.pos:DistToSqr(eye) < 1500 * 1500 then
				render.DrawWireframeSphere(l.pos, 4, 8, 8, l.color, true)
				text_at(l.pos + Vector(0, 0, 10), preview, Color(255, 220, 120))
			end
		end
		local gone = label("removed", "Removed, compile to update")
		for _, pos in pairs(removed) do
			render.DrawWireframeBox(pos, angle_zero, Vector(-12, -12, 0), Vector(12, 12, 24), Color(255, 60, 60), false)
			if pos:DistToSqr(eye) < 1500 * 1500 then text_at(pos + Vector(0, 0, 30), gone, Color(255, 90, 90)) end
		end
		local until_compile = label("ghost", "Hammer Pro: until next compile")
		for _, g in pairs(ghosts) do
			if IsValid(g) and g:GetPos():DistToSqr(eye) < 400 * 400 then
				text_at(g:GetPos() + Vector(0, 0, g:OBBMaxs().z + 6), until_compile, Color(150, 220, 255))
			end
		end
	end)

	hook.Add("HUDPaint", "hammerpro_livelink", function()
		if not active then return end
		local n = table.Count(lights) + table.Count(ghosts) + table.Count(removed)
		local text = label("hud", "Hammer Pro live link")
		if n > 0 then text = text .. "  |  " .. string.Replace(label("hud_previews", "{0} previews"), "{0}", tostring(n)) end
		draw.SimpleTextOutlined(text, "DermaDefaultBold", 16, 16, Color(255, 200, 80), TEXT_ALIGN_LEFT, TEXT_ALIGN_TOP, 1, Color(0, 0, 0, 200))
	end)
end
