-- Claude HUD for Horizon MW: our game's in-match HUD (crates/ui/src/hud_modern.rs)
-- rebuilt in H1's LUI. It replaces two of H1's HUD widgets:
--   weaponInfoHudDef  -> bottom right: gun name over a dark box with the clip in
--                        big figures and the rest of the ammo; bottom left: the
--                        player's name over a thin health bar.
--   teamScoresHudDef  -> top centre: his side's score and bar (blue), the team
--                        badges, the clock, the enemy's score and bar (red),
--                        WINNING / LOSING / TIED under it.
-- Both stay inside H1's own HUD wrappers, so H1 still hides them in menus, the
-- killcam and when the HUD is off. Every call into the game is guarded: if
-- building our widget fails, H1's stock widget is returned instead.
--
-- Turn it off by deleting this folder (ui_scripts/claude_hud).

if game:issingleplayer() or not Engine.IsMultiplayer() then
    return
end

local log = function(msg)
    pcall(function()
        print("[claude_hud] " .. tostring(msg))
    end)
end

-- H1 builds the HUD widgets from modules MPHud loads; load it so their
-- builders exist before we wrap them.
pcall(function()
    luiglobals.require("LUI.mp_hud.MPHud")
end)

-- Our palette (crates/ui/src/shell/theme.rs, hud_modern.rs).
local WHITE = { r = 1, g = 1, b = 1 }
local DIM = { r = 1, g = 1, b = 1 }
local BLUE = { r = 0.26, g = 0.62, b = 0.95 }
local RED = { r = 0.90, g = 0.26, b = 0.22 }
local LOW = { r = 0.95, g = 0.28, b = 0.22 }
local PANEL = { r = 0.04, g = 0.05, b = 0.06 }
local PANEL_ALPHA = 0.62
local TRACK = { r = 0, g = 0, b = 0 }
local TRACK_ALPHA = 0.45
local BAR_BG = { r = 0.35, g = 0.37, b = 0.40 }

-- Our HUD is laid out on a 1080-line screen; LUI's root is 720 high.
local K = 720 / 1080

local function px(v)
    return math.floor(v * K + 0.5)
end

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

local function font(name, fallback)
    local ts = CoD and CoD.TextSettings
    local t = ts and (ts[name] or ts[fallback] or ts.BodyFontBold or ts.TitleFontBold)
    return t and t.Font
end

local white_material = nil
local function white()
    if not white_material then
        white_material = luiglobals.RegisterMaterial("white")
    end
    return white_material
end

-- A solid rectangle.
local function rect(state, color, alpha)
    state.material = white()
    state.color = color
    state.alpha = alpha
    return LUI.UIImage.new(state)
end

-- A text line.
local function text(state, fontname, color, alpha, align)
    state.font = font(fontname, "TitleFontBold")
    state.color = color
    state.alpha = alpha or 1
    if align and LUI.Alignment then
        state.alignment = LUI.Alignment[align]
    end
    return LUI.UIText.new(state)
end

-- Sets an element's state to `state` (registered under a fresh name each time
-- the values change, so colours and widths can follow the game).
local function set_state(el, key, state)
    local sig = ""
    for k, v in pairs(state) do
        if type(v) == "table" then
            sig = sig .. k .. (v.r or 0) .. (v.g or 0) .. (v.b or 0)
        else
            sig = sig .. k .. tostring(v)
        end
    end
    if el["_cl_" .. key] == sig then
        return
    end
    el["_cl_" .. key] = sig
    el:registerAnimationState(key, state)
    el:animateToState(key)
end

-- A full-screen element (our widgets are laid out against the screen).
local function fullscreen()
    return LUI.UIElement.new({
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

-- Holds H1's own widget, invisible, so whatever H1 sends it still lands.
local function hide_stock(stock)
    local holder = LUI.UIElement.new({
        topAnchor = true,
        bottomAnchor = true,
        leftAnchor = true,
        rightAnchor = true,
        top = 0,
        bottom = 0,
        left = 0,
        right = 0,
        alpha = 0,
    })
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
-- Bottom right: the gun's name over the ammo box. Bottom left: name, health.
----------------------------------------------------------------------------

local CORNER_RIGHT = px(22)
local CORNER_BOTTOM = px(26)
local BOX_H = px(54)
local BOX_W = px(250)
local NAME_H = px(26)

local function build_weapon_hud()
    local root = fullscreen()

    -- The ammo box.
    local box = rect({
        rightAnchor = true,
        bottomAnchor = true,
        right = -CORNER_RIGHT,
        bottom = -CORNER_BOTTOM,
        width = BOX_W,
        height = BOX_H,
    }, PANEL, PANEL_ALPHA)
    root:addElement(box)

    -- The clip, in big figures: H1's own ammo counter (an owner draw, so it
    -- follows the gun every frame like the stock HUD's).
    local clip = text({
        rightAnchor = true,
        bottomAnchor = true,
        right = -(CORNER_RIGHT + px(92)),
        bottom = -(CORNER_BOTTOM + px(8)),
        width = px(140),
        height = px(40),
    }, "SP_HudAmmoCounterFont", WHITE, 1, "Right")
    local drawn = false
    if clip.setupOwnerdraw and CoD.Ownerdraw and CoD.Ownerdraw.CGAmmoMagazine then
        drawn = pcall(function()
            clip:setupOwnerdraw(CoD.Ownerdraw.CGAmmoMagazine, 1, CoD.TextStyle and CoD.TextStyle.Shadowed)
        end)
    end
    root:addElement(clip)

    -- The rest of the ammo.
    local stock = text({
        rightAnchor = true,
        bottomAnchor = true,
        right = -(CORNER_RIGHT + px(14)),
        bottom = -(CORNER_BOTTOM + px(14)),
        width = px(72),
        height = px(26),
    }, "SP_HudItemAmmoFont", DIM, 0.62, "Left")
    root:addElement(stock)

    -- The gun's name, right-aligned over the box.
    local gun = text({
        rightAnchor = true,
        bottomAnchor = true,
        right = -CORNER_RIGHT,
        bottom = -(CORNER_BOTTOM + BOX_H + px(2)),
        width = px(500),
        height = px(22),
    }, "SP_HudWeaponNameFont", WHITE, 1, "Right")
    root:addElement(gun)

    -- Bottom left: his name over a thin health bar.
    local name = text({
        leftAnchor = true,
        bottomAnchor = true,
        left = px(23),
        bottom = -(px(28) + px(4) + px(4)),
        width = px(400),
        height = px(20),
    }, "TitleFontBold", DIM, 0.62, "Left")
    root:addElement(name)

    local track = rect({
        leftAnchor = true,
        bottomAnchor = true,
        left = px(23),
        bottom = -px(28),
        width = px(210),
        height = math.max(2, px(4)),
    }, TRACK, TRACK_ALPHA)
    root:addElement(track)

    local fill = rect({
        leftAnchor = true,
        bottomAnchor = true,
        left = px(23),
        bottom = -px(28),
        width = px(210),
        height = math.max(2, px(4)),
    }, WHITE, 1)
    root:addElement(fill)

    local function update()
        local weapon = try("", Game.GetPlayerWeaponDisplayName)
        if type(weapon) == "string" and weapon ~= "" then
            gun:setText(Engine.ToUpperCase and try(weapon, Engine.ToUpperCase, weapon) or weapon)
        else
            gun:setText("")
        end

        local hidden = try(false, Game.IsStockAmmoHidden)
        local rest = try(nil, Game.GetPlayerStockAmmo)
        if hidden or rest == nil then
            stock:setText("")
        else
            stock:setText(tostring(rest))
            set_state(stock, "c", {
                rightAnchor = true,
                bottomAnchor = true,
                right = -(CORNER_RIGHT + px(14)),
                bottom = -(CORNER_BOTTOM + px(14)),
                width = px(72),
                height = px(26),
                color = rest == 0 and LOW or DIM,
                alpha = rest == 0 and 1 or 0.62,
            })
        end
        if not drawn then
            clip:setText("")
        end

        local who = try("", Engine.GetDvarString, "name")
        name:setText(who)

        -- His health (H1 gives 0..100; a 0..1 fraction is taken as one).
        local health = try(100, Game.GetPlayerHealth)
        if health <= 1 and health > 0 then
            health = health * 100
        end
        health = math.max(0, math.min(100, health))
        set_state(fill, "h", {
            leftAnchor = true,
            bottomAnchor = true,
            left = px(23),
            bottom = -px(28),
            width = math.floor(px(210) * health / 100 + 0.5),
            height = math.max(2, px(4)),
            color = health <= 35 and RED or WHITE,
            alpha = health > 0 and 1 or 0,
        })
    end

    every(root, 50, "claude_hud_weapon", update)
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
-- Top centre: the score bar.
----------------------------------------------------------------------------

local SIDE_BAR_W = px(260)
local SIDE_BAR_H = px(12)
local BADGE = px(58)
local CLOCK_W = px(120)
local GAP = px(8)
local TOP = px(15)

local function clock_text(ms)
    local s = math.max(0, math.floor(ms / 1000))
    return string.format("%d:%02d", math.floor(s / 60), s % 60)
end

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
    local half = CLOCK_W / 2 + GAP + BADGE + GAP

    -- One side: name over (score, bar). `dir` -1 is his side (left of the
    -- middle, the bar filling from the middle out), 1 the enemy's.
    local function side(dir, color)
        local s = {}
        local bar_left = dir < 0 and -(half + SIDE_BAR_W) or half
        local base = {
            topAnchor = true,
            left = bar_left,
            top = TOP + BADGE - SIDE_BAR_H,
            width = SIDE_BAR_W,
            height = SIDE_BAR_H,
        }
        local function centred(t)
            t.leftAnchor = false
            t.rightAnchor = false
            return t
        end
        s.bg = rect(centred({
            topAnchor = true,
            left = bar_left,
            top = base.top,
            width = SIDE_BAR_W,
            height = SIDE_BAR_H,
        }), BAR_BG, 0.75)
        s.fill = rect(centred({
            topAnchor = true,
            left = bar_left,
            top = base.top,
            width = 0,
            height = SIDE_BAR_H,
        }), color, 1)
        s.name = text(centred({
            topAnchor = true,
            left = bar_left,
            top = base.top - px(36),
            width = SIDE_BAR_W,
            height = px(22),
        }), "TitleFontBold", WHITE, 1, dir < 0 and "Right" or "Left")
        s.score = text(centred({
            topAnchor = true,
            left = dir < 0 and (bar_left - px(10) - px(80)) or (bar_left + SIDE_BAR_W + px(10)),
            top = base.top - px(6),
            width = px(80),
            height = px(22),
        }), "TitleFontBold", WHITE, 1, dir < 0 and "Right" or "Left")
        root:addElement(s.bg)
        root:addElement(s.fill)
        root:addElement(s.name)
        root:addElement(s.score)
        s.bar_left = bar_left
        s.dir = dir
        s.color = color
        return s
    end

    local function badge(left, team_dvar, color)
        local b = {}
        b.box = rect({
            topAnchor = true,
            leftAnchor = false,
            rightAnchor = false,
            left = left,
            top = TOP,
            width = BADGE,
            height = BADGE,
        }, PANEL, PANEL_ALPHA)
        root:addElement(b.box)
        local icon = try("", Engine.GetDvarString, team_dvar)
        local mat = icon ~= "" and try(nil, luiglobals.RegisterMaterial, icon) or nil
        if mat then
            b.icon = LUI.UIImage.new({
                topAnchor = true,
                leftAnchor = false,
                rightAnchor = false,
                left = left + px(3),
                top = TOP + px(3),
                width = BADGE - px(6),
                height = BADGE - px(6),
                material = mat,
            })
        else
            b.icon = rect({
                topAnchor = true,
                leftAnchor = false,
                rightAnchor = false,
                left = left + (BADGE - px(30)) / 2,
                top = TOP + (BADGE - px(30)) / 2,
                width = px(30),
                height = px(30),
            }, color, 1)
        end
        root:addElement(b.icon)
        return b
    end

    local mine = side(-1, BLUE)
    local theirs = side(1, RED)
    local my_badge, their_badge = nil, nil

    root:addElement(rect({
        topAnchor = true,
        leftAnchor = false,
        rightAnchor = false,
        left = -CLOCK_W / 2,
        top = TOP,
        width = CLOCK_W,
        height = BADGE,
    }, PANEL, PANEL_ALPHA))
    local clock = text({
        topAnchor = true,
        leftAnchor = false,
        rightAnchor = false,
        left = -CLOCK_W / 2,
        top = TOP + (BADGE - px(38)) / 2,
        width = CLOCK_W,
        height = px(38),
    }, "TitleFontBold", WHITE, 1, "Center")
    root:addElement(clock)

    local status = text({
        topAnchor = true,
        leftAnchor = false,
        rightAnchor = false,
        left = -px(200),
        top = TOP + BADGE + px(4),
        width = px(400),
        height = px(20),
    }, "TitleFontBold", DIM, 0.62, "Center")
    root:addElement(status)

    local function fill(s, score, limit)
        local part = 0
        if limit and limit > 0 then
            part = math.max(0, math.min(1, score / limit))
        end
        local w = math.floor(SIDE_BAR_W * part + 0.5)
        set_state(s.fill, "f", {
            topAnchor = true,
            leftAnchor = false,
            rightAnchor = false,
            left = s.dir < 0 and (s.bar_left + SIDE_BAR_W - w) or s.bar_left,
            top = TOP + BADGE - SIDE_BAR_H,
            width = w,
            height = SIDE_BAR_H,
            color = s.color,
            alpha = 1,
        })
    end

    local function update()
        local ffa = try(false, GameX and GameX.gameModeIsFFA)
        local limit = try(0, Engine.GetDvarInt, "ui_scorelimit")
        local yours, enemy = 0, 0
        local my_team = try("allies", Game.GetPlayerTeam)
        if ffa then
            local me = try(nil, Game.GetPlayerClientnum)
            yours = try(0, Game.GetPlayerScore, me)
            local lead = try(nil, Game.GetPlayerScoreInfoAtRank, Teams and Teams.free or "free", 1)
            local leader_name = "LEADER"
            if type(lead) == "table" then
                enemy = lead.score or 0
                if lead.name and lead.name ~= "" then
                    leader_name = try(lead.name, Engine.ToUpperCase, lead.name)
                end
            end
            mine.name:setText("YOU")
            theirs.name:setText(leader_name)
        else
            if my_team ~= "allies" and my_team ~= "axis" then
                my_team = "allies"
            end
            local other = try(my_team == "allies" and "axis" or "allies", Game.GetPlayerOpposingTeam)
            yours = try(0, Game.GetTeamScore, my_team)
            enemy = try(0, Game.GetTeamScore, other)
            mine.name:setText(team_name(my_team, my_team == "allies" and "ALLIES" or "AXIS"))
            theirs.name:setText(team_name(other, other == "allies" and "ALLIES" or "AXIS"))
        end
        mine.score:setText(tostring(yours))
        theirs.score:setText(tostring(enemy))
        fill(mine, yours, limit)
        fill(theirs, enemy, limit)

        -- The badges, once (the team is known by the first update).
        if not my_badge and not ffa then
            local my_dvar = my_team == "axis" and "g_TeamIcon_Axis" or "g_TeamIcon_Allies"
            local their_dvar = my_team == "axis" and "g_TeamIcon_Allies" or "g_TeamIcon_Axis"
            my_badge = badge(-(CLOCK_W / 2 + GAP + BADGE), my_dvar, BLUE)
            their_badge = badge(CLOCK_W / 2 + GAP, their_dvar, RED)
        elseif not my_badge then
            my_badge = badge(-(CLOCK_W / 2 + GAP + BADGE), "", BLUE)
            their_badge = badge(CLOCK_W / 2 + GAP, "", RED)
        end

        -- The clock.
        local left = try(nil, Game.GetTimeLeft)
        if type(left) == "number" and left > 0 then
            clock:setText(clock_text(left))
            set_state(clock, "c", {
                topAnchor = true,
                leftAnchor = false,
                rightAnchor = false,
                left = -CLOCK_W / 2,
                top = TOP + (BADGE - px(38)) / 2,
                width = CLOCK_W,
                height = px(38),
                color = left <= 30000 and LOW or WHITE,
                alpha = 1,
            })
        else
            clock:setText("")
        end

        local word, color, alpha = "TIED", DIM, 0.62
        if yours > enemy then
            word, color, alpha = "WINNING", BLUE, 1
        elseif yours < enemy then
            word, color, alpha = "LOSING", RED, 1
        end
        status:setText(word)
        set_state(status, "s", {
            topAnchor = true,
            leftAnchor = false,
            rightAnchor = false,
            left = -px(200),
            top = TOP + BADGE + px(4),
            width = px(400),
            height = px(20),
            color = color,
            alpha = alpha,
        })
    end

    every(root, 100, "claude_hud_scores", update)
    pcall(update)
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
log("loaded")
