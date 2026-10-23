//! The settings the MWR options menus ask for, as data: every row is a dvar
//! with a kind of control. The Legacy menu draws them and writes changes back
//! with `seta`, so they land in the same config file MWR's own menus use.

/// What kind of control a setting is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// 0 / 1.
    Toggle,
    /// A number from `min` to `max` in steps of `step`.
    Slider { min: f32, max: f32, step: f32 },
    /// One of a fixed set of `(label, dvar value)` pairs.
    Choice(&'static [(&'static str, &'static str)]),
}

#[derive(Clone, Copy, Debug)]
pub struct Setting {
    pub label: &'static str,
    pub dvar: &'static str,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug)]
pub struct Category {
    pub name: &'static str,
    pub settings: &'static [Setting],
}

const fn tog(label: &'static str, dvar: &'static str) -> Setting {
    Setting { label, dvar, kind: Kind::Toggle }
}
const fn sl(label: &'static str, dvar: &'static str, min: f32, max: f32, step: f32) -> Setting {
    Setting { label, dvar, kind: Kind::Slider { min, max, step } }
}

const AA: &[(&str, &str)] = &[("OFF", "off"), ("FXAA", "FXAA")];
const ANISO: &[(&str, &str)] = &[("OFF", "1"), ("2X", "2"), ("4X", "4"), ("8X", "8"), ("16X", "16")];
const PICMIP: &[(&str, &str)] = &[("HIGH", "0"), ("NORMAL", "1"), ("LOW", "2"), ("LOWEST", "3")];

pub const CATEGORIES: &[Category] = &[
    Category {
        name: "CONTROLS",
        settings: &[
            sl("MOUSE SENSITIVITY", "sensitivity", 0.25, 30.0, 0.25),
            tog("MOUSE ACCELERATION", "cl_mouseAccel"),
            tog("MOUSE FILTERING", "m_filter"),
            tog("FREE LOOK", "cl_freelook"),
            tog("AIM ASSIST", "aimassist_enabled"),
            sl("STICK DEADZONE MIN", "gpad_stick_deadzone_min", 0.0, 1.0, 0.05),
            sl("STICK DEADZONE MAX", "gpad_stick_deadzone_max", 0.0, 1.0, 0.05),
            sl("BUTTON DEADZONE", "gpad_button_deadzone", 0.0, 1.0, 0.05),
            tog("AUTO-DETECT GAMEPAD", "ui_autodetectGamepad"),
            tog("PUSH TO TALK", "cl_pushToTalk"),
            tog("VOICE CHAT", "cl_voice"),
            tog("TEXT CHAT", "cl_textChatEnabled"),
        ],
    },
    Category {
        name: "GRAPHICS",
        settings: &[
            tog("FULLSCREEN", "r_fullscreen"),
            tog("VSYNC", "r_vsync"),
            sl("MAX FPS", "com_maxfps", 0.0, 250.0, 5.0),
            sl("FIELD OF VIEW", "cg_fov", 65.0, 120.0, 1.0),
            sl("FOV SCALE", "cg_fovscale", 0.5, 2.0, 0.05),
            tog("SHADOWS", "sm_enable"),
            Setting { label: "ANTI-ALIASING", dvar: "r_postAA", kind: Kind::Choice(AA) },
            Setting { label: "ANISOTROPIC FILTERING", dvar: "r_texFilterAnisoMax", kind: Kind::Choice(ANISO) },
            Setting { label: "TEXTURE QUALITY", dvar: "r_picmip", kind: Kind::Choice(PICMIP) },
            tog("NORMAL MAPS", "r_normalMap"),
            tog("SPECULAR MAPS", "r_specularMap"),
            tog("DEPTH PREPASS", "r_depthPrepass"),
            tog("WATER", "r_drawWater"),
            tog("GLOW", "r_glow_allowed"),
            tog("RAGDOLL", "ragdoll_enable"),
            tog("BULLET MARKS", "fx_marks"),
            tog("LENS FLARES", "fx_flare"),
            tog("SHELL CASINGS", "cg_brass"),
            sl("RED DOT BRIGHTNESS", "r_redDotBrightnessScale", 0.2, 4.0, 0.2),
        ],
    },
    Category {
        name: "AUDIO",
        settings: &[
            sl("MASTER VOLUME", "snd_volume", 0.0, 1.0, 0.05),
            sl("CINEMATIC VOLUME", "snd_cinematicVolumeScale", 0.0, 1.0, 0.05),
            sl("MUSIC VOLUME", "legacy_music_volume", 0.0, 1.0, 0.05),
            tog("AUDIO EQ", "snd_enableEq"),
        ],
    },
    Category {
        name: "GAMEPLAY",
        settings: &[
            tog("HUD", "hud_enable"),
            tog("CROSSHAIR", "cg_drawCrosshairOption"),
            tog("HIT MARKERS", "cg_drawDamageFeedbackOption"),
            tog("XP BAR", "cg_xpbar"),
            tog("CURSOR HINTS", "cg_cursorHints"),
            tog("DESCRIPTIVE TEXT", "cg_descriptiveText"),
            tog("BREATH HINT", "cg_drawBreathHint"),
            tog("MANTLE HINT", "cg_drawMantleHint"),
            tog("DOUBLE TAP DETONATE HINT", "cg_drawDoubleTapDetonateHint"),
            sl("COMPASS SIZE", "compassSize", 0.5, 1.5, 0.1),
            sl("KILLCAM SIZE", "cg_youInKillCamSize", 0.0, 1.0, 0.1),
            sl("OBITUARY EVENTS", "cg_maxObituaryEvents", 0.0, 8.0, 1.0),
            sl("CHAT TIME", "cg_chatTime", 0.0, 30000.0, 1000.0),
            tog("TEAM CHAT ONLY", "cg_teamChatsOnly"),
            tog("DEAD CHAT", "g_deadChat"),
        ],
    },
    Category {
        name: "TELEMETRY",
        settings: &[
            tog("FPS COUNTER", "cg_infobar_fps"),
            tog("PING", "cg_infobar_ping"),
            tog("KILLSTREAK COUNTER", "cg_infobar_streak"),
            tog("DRAW FPS", "cg_drawFps"),
            tog("DRAW PING", "cg_drawping"),
            sl("PACKETS PER SECOND", "cl_maxpackets", 15.0, 125.0, 5.0),
        ],
    },
];

fn truthy(v: &str) -> bool {
    let v = v.trim();
    v.parse::<f32>().map(|n| n != 0.0).unwrap_or_else(|_| v.eq_ignore_ascii_case("true"))
}

fn decimals(step: f32) -> usize {
    if step >= 1.0 { 0 } else if step >= 0.1 { 1 } else { 2 }
}

impl Setting {
    /// The next value after `dir` (-1 / +1) steps from `current`, as dvar text.
    pub fn step(&self, current: &str, dir: i32) -> String {
        match self.kind {
            Kind::Toggle => if truthy(current) { "0" } else { "1" }.to_owned(),
            Kind::Slider { min, max, step } => {
                let now = current.trim().parse::<f32>().unwrap_or(min);
                let n = ((now - min) / step).round() + dir as f32;
                let v = (min + n * step).clamp(min, max);
                format!("{:.*}", decimals(step), v)
            }
            Kind::Choice(opts) => {
                let at = opts.iter().position(|(_, v)| v.eq_ignore_ascii_case(current.trim())).unwrap_or(0) as i32;
                let n = opts.len() as i32;
                opts[(at + dir).rem_euclid(n) as usize].1.to_owned()
            }
        }
    }

    /// The text shown for `current`, and a 0..1 fill when it is a slider.
    pub fn show(&self, current: &str) -> (String, Option<f32>) {
        match self.kind {
            Kind::Toggle => (if truthy(current) { "ON" } else { "OFF" }.to_owned(), None),
            Kind::Slider { min, max, step } => {
                let now = current.trim().parse::<f32>().unwrap_or(min).clamp(min, max);
                (format!("{:.*}", decimals(step), now), Some((now - min) / (max - min)))
            }
            Kind::Choice(opts) => {
                let t = current.trim();
                let label = opts.iter().find(|(_, v)| v.eq_ignore_ascii_case(t)).map_or(t, |(l, _)| *l);
                (label.to_owned(), None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_flip_and_read_text() {
        let s = tog("X", "x");
        assert_eq!(s.step("0", 1), "1");
        assert_eq!(s.step("1", -1), "0");
        assert_eq!(s.show("1").0, "ON");
        assert_eq!(s.show("0").0, "OFF");
    }

    #[test]
    fn sliders_step_clamp_and_fill() {
        let s = sl("X", "x", 65.0, 120.0, 1.0);
        assert_eq!(s.step("65", -1), "65");
        assert_eq!(s.step("119", 1), "120");
        assert_eq!(s.step("120", 1), "120");
        assert_eq!(s.step("junk", 1), "66");
        assert_eq!(s.show("120").1, Some(1.0));
        let f = sl("Y", "y", 0.2, 4.0, 0.2);
        assert_eq!(f.step("1.0", 1), "1.2");
    }

    #[test]
    fn choices_wrap_and_show_labels() {
        let s = Setting { label: "A", dvar: "a", kind: Kind::Choice(ANISO) };
        assert_eq!(s.step("16", 1), "1");
        assert_eq!(s.step("1", -1), "16");
        assert_eq!(s.show("4").0, "4X");
        assert_eq!(s.show("weird").0, "weird");
    }

    #[test]
    fn every_dvar_is_unique_and_sliders_are_sane() {
        let mut seen = std::collections::HashSet::new();
        for c in CATEGORIES {
            for s in c.settings {
                assert!(seen.insert(s.dvar), "duplicate {}", s.dvar);
                if let Kind::Slider { min, max, step } = s.kind {
                    assert!(min < max && step > 0.0 && step <= max - min, "{}", s.dvar);
                }
            }
        }
    }
}
