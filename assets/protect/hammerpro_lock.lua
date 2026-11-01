local M = {}

local function bxor32(a, b)
	local r = bit.bxor(a, b)
	if r < 0 then r = r + 4294967296 end
	return r
end
local function fnv(s)
	local h = 2166136261
	for i = 1, #s do
		h = bxor32(h, string.byte(s, i))
		local hi = math.floor(h / 65536)
		local lo = h % 65536
		h = ((lo * 16777619) % 4294967296 + ((hi * 16777619) % 65536) * 65536) % 4294967296
	end
	return string.format("%08x", h)
end

local function read_map_keys()
	local out = {}
	local f = file.Open("maps/" .. game.GetMap() .. ".bsp", "rb", "GAME")
	if not f then return out end
	f:Seek(8)
	local off = f:ReadLong()
	local len = f:ReadLong()
	if off and len and off > 0 and len > 0 then
		f:Seek(off)
		local text = f:Read(math.min(len, 1048576)) or ""
		for _, key in ipairs({ "hammerpro_lock", "hammerpro_lock_block", "hammerpro_protected" }) do
			out[key] = string.match(text, '"' .. key .. '"%s*"([^"]*)"')
		end
	end
	f:Close()
	return out
end

if SERVER then
	util.AddNetworkString("hp_lock")

	CreateConVar("hammerpro_key", "", FCVAR_ARCHIVE, "Protect your BSP server key")

	M.keys = read_map_keys()
	M.authorized = (not M.keys.hammerpro_lock) or (M.keys.hammerpro_lock == "")

	local function refresh()
		if not M.keys.hammerpro_lock or M.keys.hammerpro_lock == "" then
			M.authorized = true
			return
		end
		M.authorized = fnv(GetConVar("hammerpro_key"):GetString()) == M.keys.hammerpro_lock
	end
	refresh()
	cvars.AddChangeCallback("hammerpro_key", function()
		refresh()
		for _, ply in ipairs(player.GetAll()) do
			M.send(ply)
		end
	end, "hammerpro_lock")

	function M.send(ply)
		net.Start("hp_lock")
		net.WriteBool(M.authorized)
		net.WriteString(string.sub(M.keys.hammerpro_protected or "", 1, 64))
		net.WriteBool(M.keys.hammerpro_lock_block == "1")
		net.Send(ply)
	end

	hook.Add("PlayerInitialSpawn", "hammerpro_lock", function(ply)
		timer.Simple(1, function()
			if IsValid(ply) then M.send(ply) end
		end)
	end)
	return
end

local state = { authorized = true, author = "", block = false }

net.Receive("hp_lock", function()
	state.authorized = net.ReadBool()
	state.author = net.ReadString()
	state.block = net.ReadBool()
end)

surface.CreateFont("HammerProLock", { font = "Trebuchet MS", size = 26, weight = 800 })
surface.CreateFont("HammerProLockBig", { font = "Trebuchet MS", size = 40, weight = 800 })

hook.Add("HUDPaint", "hammerpro_lock", function()
	if state.authorized then return end
	local w, h = ScrW(), ScrH()
	local tag = "Protected by " .. (state.author ~= "" and state.author or "Protect your BSP")

	surface.SetFont("HammerProLock")
	local tw = surface.GetTextSize(tag)
	for y = 0, h, 140 do
		for x = -tw, w, tw + 120 do
			draw.SimpleText(tag, "HammerProLock", x + (y / 140 % 2) * 80, y, Color(255, 255, 255, 18))
		end
	end
	if state.block then
		draw.RoundedBox(6, 0, h / 2 - 60, w, 120, Color(0, 0, 0, 210))
		draw.SimpleText(tag, "HammerProLockBig", w / 2, h / 2 - 20, Color(255, 80, 80), TEXT_ALIGN_CENTER, TEXT_ALIGN_CENTER)
		draw.SimpleText("Unauthorized server: this map is not allowed to run here.", "HammerProLock", w / 2, h / 2 + 24, Color(230, 230, 230), TEXT_ALIGN_CENTER, TEXT_ALIGN_CENTER)
	end
end)

hook.Add("StartCommand", "hammerpro_lock", function(_, cmd)
	if state.authorized or not state.block then return end
	cmd:ClearMovement()
	cmd:ClearButtons()
end)
