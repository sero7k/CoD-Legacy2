-- Claude HUD for Horizon MW: the in-match HUD is drawn by Rust
-- (rust/codlegacy/src/ui.rs, through the engine's 2D commands). This script
-- only reads the game's values and hands them to Rust (game:rustset), and
-- hides H1's own HUD widgets that Rust replaces:
--   weaponInfoHudDef     -> gun name, ammo, health, his name, scorestreaks
--   teamScoresHudDef     -> scores, clock, team names, score limit
--   obituary             -> the kill feed
--   damageFeedbackHudDef -> the hit marker
-- Everything else of H1's HUD (minimap, splashes, the
-- scoreboard) is left alone. Each stock widget stays in the tree, hidden, so
-- whatever H1 sends it still lands, and H1 still hides the HUD in menus, the
-- killcam and when the HUD is off. Every call into the game is guarded: if
-- hiding a widget fails, H1's own stays visible.
--
-- Built into the client as resource CLAUDE_HUD.

do
    local ok, mp = pcall(function()
        return Engine.IsMultiplayer()
    end)
    if not ok or not mp then
        return
    end
end

local log = function(msg)
    pcall(function()
        local line = "[claude_hud] " .. tostring(msg)
        if game and game.claudelog then
            game:claudelog(line)
        end
        print(line)
    end)
end

if not (game and game.rustset) then
    log("no game:rustset: H1's HUD kept")
    return
end

-- H1 builds the HUD widgets from modules MPHud loads; load it so their
-- builders exist before we wrap them.
pcall(function()
    luiglobals.require("LUI.mp_hud.MPHud")
end)

-- A value from the game, or `fallback` when the call is missing or fails.
local function try(fallback, fn, ...)
    if type(fn) ~= "function" then
        return fallback
    end
    local ok, v = pcall(fn, ...)
    if ok and v ~= nil then
        return v
    end
    return fallback
end

-- One value to Rust, as text.
local function push(key, value)
    pcall(function()
        game:rustset(key, tostring(value == nil and "" or value))
    end)
end

local function fullscreen(alpha)
    return LUI.UIElement.new({
        alpha = alpha,
        topAnchor = true,
        bottomAnchor = true,
        leftAnchor = true,
        rightAnchor = true,
        top = 0,
        bottom = 0,
        left = 0,
        right = 0,
    })
end

-- Holds H1's own widget, invisible.
local function hide_stock(stock)
    local holder = fullscreen(0)
    holder:addElement(stock)
    return holder
end

local function every(el, ms, name, fn)
    el:addElement(LUI.UITimer.new(ms, name))
    el:registerEventHandler(name, function()
        local ok, err = pcall(fn)
        if not ok then
            log(err)
        end
    end)
end

----------------------------------------------------------------------------
-- Weapon, ammo, health.
----------------------------------------------------------------------------

-- The magazine count: H1 draws it as an engine owner draw, so the script API
-- may or may not offer it. Candidates are tried once; the one that answers is
-- logged.
local CLIP_CANDIDATES = {
    "GetPlayerAmmoInClip",
    "GetPlayerClipAmmo",
    "GetPlayerWeaponClipAmmo",
    "GetPlayerAmmoClip",
    "GetWeaponAmmoClip",
    "GetPlayerWeaponAmmoClip",
}
local clip_fn, clip_checked = nil, false

local function clip_ammo()
    if not clip_checked then
        clip_checked = true
        for _, name in ipairs(CLIP_CANDIDATES) do
            local fn = Game and Game[name]
            if type(fn) == "function" then
                local ok, v = pcall(fn)
                if ok and type(v) == "number" then
                    clip_fn = fn
                    log("clip ammo from Game." .. name)
                    break
                end
            end
        end
        if not clip_fn then
            log("no clip ammo call in Game: the clip count is left out")
        end
    end
    if clip_fn then
        local ok, v = pcall(clip_fn)
        if ok and type(v) == "number" then
            return v
        end
    end
    return nil
end

-- His scorestreaks (H1's ks_icon_1..3 omnvars, ks_hasStreak's bits), as
-- "material:earned" for Rust.
local function streak_material(idx)
    local tbl = KillstreakTable
    if type(tbl) ~= "table" or not tbl.Cols then
        return nil
    end
    local name = try("", Engine.TableLookup, tbl.File, tbl.Cols.Index, tostring(idx), tbl.Cols.EarnedIcon)
    if type(name) ~= "string" or name == "" or name == "specialty_ks_null" then
        return nil
    end
    return name
end

local function has_streak(i)
    local bits = try(0, Game.GetOmnvar, "ks_hasStreak")
    if type(bits) ~= "number" then
        return false
    end
    return math.floor(bits / (2 ^ i)) % 2 == 1
end

local function push_streaks()
    local slots = {}
    for i = 1, 3 do
        local idx = try(0, Game.GetOmnvar, "ks_icon_" .. i)
        local mat = type(idx) == "number" and idx > 0 and streak_material(idx) or nil
        if mat then
            table.insert(slots, mat:gsub("[,:|]", "_") .. ":" .. (has_streak(i) and "1" or "0"))
        else
            table.insert(slots, "-")
        end
    end
    push("streaks", table.concat(slots, ","))
end

local function build_weapon_hud()
    local root = fullscreen()
    local function update()
        push("beat", 1)
        local weapon = try("", Game.GetPlayerWeaponDisplayName)
        if type(weapon) == "string" and weapon ~= "" and Engine.ToUpperCase then
            weapon = try(weapon, Engine.ToUpperCase, weapon)
        end
        push("weapon", weapon)
        local hidden = try(false, Game.IsStockAmmoHidden)
        local rest = try(nil, Game.GetPlayerStockAmmo)
        push("stock", (hidden or rest == nil) and "" or rest)
        local clip = clip_ammo()
        push("clip", clip == nil and "" or clip)
        push("name", try("", Engine.GetDvarString, "name"))
        -- His health (H1 gives 0..100; a 0..1 fraction is taken as one).
        local health = try(100, Game.GetPlayerHealth)
        if type(health) == "number" and health <= 1 and health > 0 then
            health = health * 100
        end
        push("health", health)
    end
    every(root, 50, "claude_hud_weapon", update)
    every(root, 250, "claude_hud_streaks", push_streaks)
    root:registerEventHandler("weapon_change", function()
        pcall(update)
    end)
    root:registerEventHandler("playerstate_client_changed", function()
        pcall(update)
    end)
    pcall(update)
    return root
end

----------------------------------------------------------------------------
-- Scores and the clock.
----------------------------------------------------------------------------

local function team_name(team, fallback)
    local key = team == "axis" and "g_TeamName_Axis" or "g_TeamName_Allies"
    local raw = try("", Engine.GetDvarString, key)
    if raw == "" then
        return fallback
    end
    local loc = try(raw, Engine.Localize, raw)
    return try(loc, Engine.ToUpperCase, loc)
end

local function build_score_hud()
    local root = fullscreen()
    local function update()
        local ffa = try(false, GameX and GameX.gameModeIsFFA)
        push("ffa", ffa and 1 or 0)
        push("limit", try(0, Engine.GetDvarInt, "ui_scorelimit"))
        local yours, enemy = 0, 0
        local mine_name, their_name
        if ffa then
            local me = try(nil, Game.GetPlayerClientnum)
            yours = try(0, Game.GetPlayerScore, me)
            local lead = try(nil, Game.GetPlayerScoreInfoAtRank, Teams and Teams.free or "free", 1)
            their_name = "LEADER"
            if type(lead) == "table" then
                enemy = lead.score or 0
                if lead.name and lead.name ~= "" then
                    their_name = try(lead.name, Engine.ToUpperCase, lead.name)
                end
            end
            mine_name = "YOU"
        else
            local my_team = try("allies", Game.GetPlayerTeam)
            if my_team ~= "allies" and my_team ~= "axis" then
                my_team = "allies"
            end
            local other = try(my_team == "allies" and "axis" or "allies", Game.GetPlayerOpposingTeam)
            yours = try(0, Game.GetTeamScore, my_team)
            enemy = try(0, Game.GetTeamScore, other)
            mine_name = team_name(my_team, my_team == "allies" and "ALLIES" or "AXIS")
            their_name = team_name(other, other == "allies" and "ALLIES" or "AXIS")
        end
        push("mine", yours)
        push("theirs", enemy)
        push("mine_name", mine_name)
        push("theirs_name", their_name)
        local left = try(nil, Game.GetTimeLeft)
        push("clock", (type(left) == "number" and left > 0) and math.floor(left) or "")
    end
    every(root, 100, "claude_hud_scores", update)
    pcall(update)
    return root
end

----------------------------------------------------------------------------
-- The kill feed.
----------------------------------------------------------------------------

local WHITE_HEX, GOLD_HEX, RED_HEX, BLUE_HEX = "ffffff", "fab84d", "e64238", "429ef2"

local obituary_module = nil
local function find_obituary()
    if obituary_module == nil then
        obituary_module = false
        pcall(function()
            for _, v in pairs(LUI.mp_hud) do
                if type(v) == "table" and type(rawget(v, "getColorForClientNum")) == "function" then
                    obituary_module = v
                end
            end
        end)
    end
    return obituary_module or nil
end

local function name_color(cn)
    if type(cn) ~= "number" or cn < 0 then
        return WHITE_HEX
    end
    local me = try(nil, Game.GetPlayerClientnum)
    if me ~= nil and cn == me then
        return GOLD_HEX
    end
    local hud = Swatches and Swatches.HUD
    local m = find_obituary()
    if m and hud then
        local team = try(nil, Game.GetPlayerTeam)
        local ok, c = pcall(m.getColorForClientNum, cn, team)
        if ok and c ~= nil then
            if c == hud.Me then
                return GOLD_HEX
            elseif c == hud.Enemies then
                return RED_HEX
            elseif c == hud.Allies then
                return BLUE_HEX
            end
        end
    end
    if try(false, GameX and GameX.gameModeIsFFA) then
        return RED_HEX
    end
    return WHITE_HEX
end

local function player_name(cn)
    if type(cn) ~= "number" or cn < 0 then
        return ""
    end
    local n = try("", Game.GetPlayerName, cn)
    return type(n) == "string" and n or ""
end

-- Text parts must not hold the separator.
local function clean(s)
    return (tostring(s):gsub("|", " "))
end

local function build_feed()
    local root = fullscreen()
    root:registerEventHandler("obituary", function(_, event)
        pcall(function()
            if type(event) ~= "table" then
                return
            end
            local parts = {}
            local attacker, victim = event.attacker, event.victim
            if attacker ~= nil and attacker ~= victim then
                local a = player_name(attacker)
                if a ~= "" then
                    table.insert(parts, "T:" .. name_color(attacker) .. ":" .. clean(a))
                end
            end
            if event.icon ~= nil then
                table.insert(parts, "I:" .. clean(event.icon))
            else
                table.insert(parts, "T:ffffff:-")
            end
            local v = player_name(victim)
            if v ~= "" then
                table.insert(parts, "T:" .. name_color(victim) .. ":" .. clean(v))
            end
            push("feed", table.concat(parts, "|"))
        end)
    end)
    root:registerEventHandler("game_message", function(_, event)
        pcall(function()
            if type(event) == "table" and type(event.message) == "string" and event.message ~= "" then
                push("feed", "T:ffffff:" .. clean(event.message))
            end
        end)
    end)
    return root
end

----------------------------------------------------------------------------
-- The hit marker.
----------------------------------------------------------------------------

local function build_hitmarker()
    local root = fullscreen()
    root:registerOmnvarHandler("damage_feedback", function(_, event)
        pcall(function()
            local v = type(event) == "table" and event.value or nil
            if v == nil then
                v = try(nil, Game.GetOmnvar, "damage_feedback")
            end
            if v == nil or v == "" or v == 0 then
                return
            end
            push("hit", string.find(tostring(v), "killshot", 1, true) and "kill" or "hit")
        end)
    end)
    return root
end

----------------------------------------------------------------------------
-- Wrapping H1's widget builders.
----------------------------------------------------------------------------

local function wrap(type_name, build_ours)
    local builders = LUI.MenuBuilder and LUI.MenuBuilder.m_types_build
    local stock_builder = builders and builders[type_name]
    if not stock_builder then
        log("no " .. type_name .. ": left as it is")
        return
    end
    builders[type_name] = function(...)
        local stock = stock_builder(...)
        local ok, ours = pcall(build_ours)
        if not ok or not ours then
            log(type_name .. ": " .. tostring(ours) .. " (stock HUD kept)")
            return stock
        end
        local ok2, holder = pcall(hide_stock, stock)
        if not ok2 then
            log(type_name .. ": " .. tostring(holder) .. " (stock HUD kept)")
            return stock
        end
        ours:addElement(holder)
        return ours
    end
end

wrap("weaponInfoHudDef", build_weapon_hud)
wrap("teamScoresHudDef", build_score_hud)
wrap("obituary", build_feed)
wrap("damageFeedbackHudDef", build_hitmarker)
log("loaded: " .. tostring(game:rustversion()))
