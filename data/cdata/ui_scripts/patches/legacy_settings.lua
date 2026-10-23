-- MWR's own settings screens are replaced by the Legacy menu (Rust overlay).
-- Opening the settings hub opens the overlay and strips the hub down to one
-- button that reopens it. Everything is guarded: if anything here fails, the
-- game's own menu is left as it was.

if game:issingleplayer() then
    return
end

LUI.onmenuopen("pc_controls", function(menu)
    local ok = pcall(function()
        if not menu.list then
            return
        end

        local buttonlist = menu:getChildById(menu.type .. "_list")
        for i = 1, 40 do
            local button = buttonlist.childRecord[string.format("%s_button_%s", menu.type, i)]
            if button then
                buttonlist:removeElement(button)
            end
        end

        menu:AddButton("LEGACY SETTINGS", function()
            Engine.Exec("legacy_menu")
        end, nil, true, nil, {
            desc_text = "Controls, graphics, audio, gameplay and maps"
        })

        LUI.Options.InitScrollingList(menu.list, nil)
    end)

    if ok then
        Engine.Exec("legacy_menu")
    end
end)
