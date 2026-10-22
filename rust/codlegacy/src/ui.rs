//! CoD Legacy's in-match HUD, drawn from Rust.
//!
//! The client calls [`codlegacy_ui_frame`] once a frame (from the renderer's
//! 2D pass) with a table of drawing functions of the engine's; this module
//! lays the HUD out on a 1080-line screen and draws it through them. What it
//! shows comes from Lua through [`codlegacy_ui_set`] (the game's own HUD
//! scripts read the values and push them here, then hide H1's widgets):
//!
//! | key        | value                                                       |
//! |------------|-------------------------------------------------------------|
//! | `beat`     | any: the HUD is up (nothing is drawn when it goes stale)     |
//! | `weapon`   | the gun's display name                                      |
//! | `clip`     | rounds in the magazine (empty: not known)                   |
//! | `stock`    | the rest of the ammo (empty: hidden)                        |
//! | `health`   | 0..100                                                      |
//! | `name`     | his name                                                    |
//! | `ffa`      | `1` in a free-for-all (no sides)                            |
//! | `mine`/`theirs`            | the two scores                              |
//! | `mine_name`/`theirs_name`  | what stands over each score                 |
//! | `limit`    | the score limit                                             |
//! | `clock`    | ms left (empty: none)                                       |
//! | `feed`     | a kill feed row: parts joined by `\|`, each `T:rrggbb:text` or `I:material` |
//! | `hit`      | `hit` or `kill`: the hit marker                              |

use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The engine's drawing functions, filled in by the client. Colours are four
/// floats (r, g, b, a); `y` of a text is its baseline; sizes are pixels.
#[repr(C)]
pub struct UiApi {
    pub fill_rect: extern "C" fn(x: f32, y: f32, w: f32, h: f32, rgba: *const f32),
    pub draw_text: extern "C" fn(text: *const c_char, x: f32, y: f32, size: i32, rgba: *const f32),
    pub text_width: extern "C" fn(text: *const c_char, size: i32) -> f32,
    pub draw_image: extern "C" fn(material: *const c_char, x: f32, y: f32, w: f32, h: f32, rgba: *const f32),
}

/// What the HUD draws on (the engine in the game, a recorder in tests).
pub trait Surface {
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]);
    /// `y` is the baseline.
    fn text(&mut self, s: &str, x: f32, y: f32, size: i32, c: [f32; 4]);
    fn width(&mut self, s: &str, size: i32) -> f32;
    fn image(&mut self, material: &str, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]);
}

struct Engine<'a>(&'a UiApi);

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', " ")).unwrap_or_default()
}

impl Surface for Engine<'_> {
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
        (self.0.fill_rect)(x, y, w, h, c.as_ptr());
    }
    fn text(&mut self, s: &str, x: f32, y: f32, size: i32, c: [f32; 4]) {
        let s = cstring(s);
        (self.0.draw_text)(s.as_ptr(), x, y, size, c.as_ptr());
    }
    fn width(&mut self, s: &str, size: i32) -> f32 {
        let s = cstring(s);
        (self.0.text_width)(s.as_ptr(), size)
    }
    fn image(&mut self, material: &str, x: f32, y: f32, w: f32, h: f32, c: [f32; 4]) {
        let m = cstring(material);
        (self.0.draw_image)(m.as_ptr(), x, y, w, h, c.as_ptr());
    }
}

// Palette (crates/ui/src/hud_modern.rs).
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const DIM: [f32; 4] = [1.0, 1.0, 1.0, 0.62];
const BLUE: [f32; 4] = [0.26, 0.62, 0.95, 1.0];
const RED: [f32; 4] = [0.90, 0.26, 0.22, 1.0];
const PANEL: [f32; 4] = [0.04, 0.05, 0.06, 0.62];
const TRACK: [f32; 4] = [0.0, 0.0, 0.0, 0.45];
const HIT_RED: [f32; 4] = [0.96, 0.16, 0.12, 1.0];

/// How long the HUD stays up after the last `beat`.
const BEAT_LIFE: Duration = Duration::from_millis(700);
const FEED_ROWS: usize = 5;
const FEED_LIFE: Duration = Duration::from_millis(6000);
const FEED_OUT: Duration = Duration::from_millis(600);
const HIT_LIFE: Duration = Duration::from_millis(300);
const KILL_LIFE: Duration = Duration::from_millis(420);

#[derive(Clone, Debug, PartialEq)]
enum Part {
    Text(String, [f32; 4]),
    Image(String),
}

struct Row {
    born: Instant,
    parts: Vec<Part>,
}

#[derive(Default)]
pub struct Hud {
    values: HashMap<String, String>,
    beat: Option<Instant>,
    feed: Vec<Row>,
    hit: Option<(Instant, bool)>,
}

fn parse_colour(hex: &str) -> [f32; 4] {
    let n = u32::from_str_radix(hex, 16).unwrap_or(0xffffff);
    [((n >> 16) & 255) as f32 / 255.0, ((n >> 8) & 255) as f32 / 255.0, (n & 255) as f32 / 255.0, 1.0]
}

fn parse_feed(value: &str) -> Vec<Part> {
    value
        .split('|')
        .filter_map(|p| {
            if let Some(m) = p.strip_prefix("I:") {
                (!m.is_empty()).then(|| Part::Image(m.to_owned()))
            } else if let Some(rest) = p.strip_prefix("T:") {
                let (hex, text) = rest.split_once(':')?;
                (!text.is_empty()).then(|| Part::Text(text.to_owned(), parse_colour(hex)))
            } else {
                None
            }
        })
        .collect()
}

fn with_alpha(mut c: [f32; 4], a: f32) -> [f32; 4] {
    c[3] *= a;
    c
}

impl Hud {
    pub fn set(&mut self, key: &str, value: &str, now: Instant) {
        match key {
            "beat" => self.beat = Some(now),
            "feed" => {
                let parts = parse_feed(value);
                if !parts.is_empty() {
                    self.feed.push(Row { born: now, parts });
                    let extra = self.feed.len().saturating_sub(FEED_ROWS);
                    self.feed.drain(..extra);
                }
            }
            "hit" => {
                let kill = value == "kill";
                // (A hit just after the kill's X keeps it red.)
                let keeps = !kill && matches!(self.hit, Some((t, true)) if now.duration_since(t) < Duration::from_millis(120));
                if !keeps && (kill || value == "hit") {
                    self.hit = Some((now, kill));
                }
            }
            _ => {
                self.values.insert(key.to_owned(), value.to_owned());
            }
        }
    }

    fn get(&self, key: &str) -> &str {
        self.values.get(key).map_or("", String::as_str)
    }

    fn num(&self, key: &str) -> Option<f32> {
        self.get(key).trim().parse().ok()
    }

    /// Whether the HUD is up at `now`.
    pub fn up(&self, now: Instant) -> bool {
        self.beat.is_some_and(|b| now.duration_since(b) <= BEAT_LIFE)
    }

    /// Draws one frame on a `w` x `h` screen.
    pub fn draw(&mut self, s: &mut dyn Surface, w: f32, h: f32, now: Instant) {
        if !self.up(now) {
            return;
        }
        self.feed.retain(|r| now.duration_since(r.born) < FEED_LIFE + FEED_OUT);
        let k = h / 1080.0;
        self.draw_scores(s, w, k);
        self.draw_gun(s, w, h, k);
        self.draw_feed(s, h, k, now);
        self.draw_hit(s, w, h, k, now);
    }

    fn draw_scores(&self, s: &mut dyn Surface, w: f32, k: f32) {
        const BAR_W: f32 = 260.0;
        const BAR_H: f32 = 12.0;
        const BADGE: f32 = 58.0;
        const CLOCK_W: f32 = 120.0;
        const GAP: f32 = 8.0;
        const TOP: f32 = 15.0;
        let cx = w / 2.0;
        let (mine, theirs) = (self.num("mine").unwrap_or(0.0), self.num("theirs").unwrap_or(0.0));
        let limit = self.num("limit").unwrap_or(0.0);
        let part = |score: f32| if limit > 0.0 { (score / limit).clamp(0.0, 1.0) } else { 0.0 };

        // The clock between them.
        s.fill(cx - CLOCK_W / 2.0 * k, TOP * k, CLOCK_W * k, BADGE * k, PANEL);
        if let Some(ms) = self.num("clock").filter(|ms| *ms > 0.0) {
            let secs = (ms / 1000.0) as i64;
            let text = format!("{}:{:02}", secs / 60, secs % 60);
            let size = (38.0 * k) as i32;
            let tw = s.width(&text, size);
            let colour = if ms <= 30000.0 { [0.95, 0.28, 0.22, 1.0] } else { WHITE };
            s.text(&text, cx - tw / 2.0, (TOP + BADGE / 2.0 + 13.0) * k, size, colour);
        }

        // Each side: its bar (growing away from the clock) with the score and
        // its name over it.
        let bar_y = (TOP + BADGE - BAR_H) * k;
        let edge = (CLOCK_W / 2.0 + GAP) * k;
        for (left, score, colour, name) in [(true, mine, BLUE, self.get("mine_name")), (false, theirs, RED, self.get("theirs_name"))] {
            let bar_w = BAR_W * k;
            let x0 = if left { cx - edge - bar_w } else { cx + edge };
            s.fill(x0, bar_y, bar_w, BAR_H * k, TRACK);
            let fill_w = (bar_w * part(score)).round();
            let fx = if left { x0 + bar_w - fill_w } else { x0 };
            s.fill(fx, bar_y, fill_w, BAR_H * k, colour);
            let big = (34.0 * k) as i32;
            let small = (18.0 * k) as i32;
            let number = format!("{}", score as i64);
            let nw = s.width(&number, big);
            let name_w = s.width(name, small);
            if left {
                s.text(&number, x0 + bar_w - nw, bar_y - 6.0 * k, big, WHITE);
                s.text(name, x0 + bar_w - nw - 10.0 * k - name_w, bar_y - 6.0 * k, small, DIM);
            } else {
                s.text(&number, x0, bar_y - 6.0 * k, big, WHITE);
                s.text(name, x0 + nw + 10.0 * k, bar_y - 6.0 * k, small, DIM);
            }
        }

        // WINNING / LOSING / TIED under the clock.
        let (word, colour) = if mine > theirs {
            ("WINNING", BLUE)
        } else if mine < theirs {
            ("LOSING", RED)
        } else {
            ("TIED", DIM)
        };
        let size = (18.0 * k) as i32;
        let ww = s.width(word, size);
        s.text(word, cx - ww / 2.0, (TOP + BADGE + 22.0) * k, size, colour);
    }

    fn draw_gun(&self, s: &mut dyn Surface, w: f32, h: f32, k: f32) {
        const RIGHT: f32 = 22.0;
        const BOTTOM: f32 = 26.0;
        const BOX_W: f32 = 250.0;
        const BOX_H: f32 = 54.0;
        let box_x = w - (RIGHT + BOX_W) * k;
        let box_y = h - (BOTTOM + BOX_H) * k;
        s.fill(box_x, box_y, BOX_W * k, BOX_H * k, PANEL);

        // The clip in big figures, the rest of the ammo small beside it.
        let clip = self.get("clip");
        if !clip.is_empty() {
            let size = (44.0 * k) as i32;
            let cw = s.width(clip, size);
            s.text(clip, w - (RIGHT + 92.0) * k - cw, h - (BOTTOM + 10.0) * k, size, WHITE);
        }
        let stock = self.get("stock");
        if !stock.is_empty() {
            let empty = stock.trim() == "0";
            let colour = if empty { [0.95, 0.28, 0.22, 1.0] } else { DIM };
            s.text(stock, w - (RIGHT + 86.0) * k + 0.0, h - (BOTTOM + 16.0) * k, (24.0 * k) as i32, colour);
        }

        // The gun's name over the box, right-aligned.
        let weapon = self.get("weapon");
        if !weapon.is_empty() {
            let size = (22.0 * k) as i32;
            let ww = s.width(weapon, size);
            s.text(weapon, w - RIGHT * k - ww, box_y - 8.0 * k, size, WHITE);
        }

        // Bottom left: his name over a thin health bar.
        let name = self.get("name");
        if !name.is_empty() {
            s.text(name, 23.0 * k, h - 40.0 * k, (20.0 * k) as i32, DIM);
        }
        let health = self.num("health").unwrap_or(100.0);
        let health = if health > 0.0 && health <= 1.0 { health * 100.0 } else { health }.clamp(0.0, 100.0);
        let (bx, by, bw, bh) = (23.0 * k, h - 32.0 * k, 210.0 * k, (4.0 * k).max(2.0));
        s.fill(bx, by, bw, bh, TRACK);
        if health > 0.0 {
            s.fill(bx, by, (bw * health / 100.0).round(), bh, if health <= 35.0 { RED } else { WHITE });
        }
    }

    fn draw_feed(&self, s: &mut dyn Surface, h: f32, k: f32, now: Instant) {
        const TEXT: f32 = 22.0;
        const ROW_H: f32 = TEXT + 6.0;
        const GAP: f32 = 3.0;
        const BOTTOM: f32 = 72.0;
        const LEFT: f32 = 23.0;
        let size = (TEXT * k) as i32;
        let n = self.feed.len();
        for (i, row) in self.feed.iter().enumerate() {
            let age = now.duration_since(row.born);
            let fade = if age > FEED_LIFE {
                1.0 - ((age - FEED_LIFE).as_secs_f32() / FEED_OUT.as_secs_f32()).min(1.0)
            } else {
                1.0
            };
            let from_bottom = (n - 1 - i) as f32;
            let top = h - (BOTTOM + ROW_H + from_bottom * (ROW_H + GAP)) * k;
            // Its width first, for the strip behind it.
            let pad = 8.0 * k;
            let gap = 7.0 * k;
            let icon_h = TEXT * 1.05 * k;
            let mut width = 0.0;
            let mut widths = Vec::with_capacity(row.parts.len());
            for p in &row.parts {
                let pw = match p {
                    Part::Text(t, _) => s.width(t, size),
                    Part::Image(_) => icon_h * 3.2,
                };
                widths.push(pw);
                width += pw + gap;
            }
            let width = (width - gap).max(0.0);
            s.fill(LEFT * k, top, width + pad * 2.0, ROW_H * k, with_alpha([0.04, 0.05, 0.06, 0.42], fade));
            let mut x = LEFT * k + pad;
            for (p, pw) in row.parts.iter().zip(widths) {
                match p {
                    Part::Text(t, c) => s.text(t, x, top + (ROW_H * k + TEXT * k * 0.7) / 2.0, size, with_alpha(*c, fade)),
                    Part::Image(m) => s.image(m, x, top + (ROW_H * k - icon_h) / 2.0, pw, icon_h, with_alpha(WHITE, fade)),
                }
                x += pw + gap;
            }
        }
    }

    fn draw_hit(&self, s: &mut dyn Surface, w: f32, h: f32, k: f32, now: Instant) {
        let Some((at, kill)) = self.hit else { return };
        let life = if kill { KILL_LIFE } else { HIT_LIFE };
        let age = now.duration_since(at);
        if age >= life {
            return;
        }
        let t = age.as_secs_f32() / life.as_secs_f32();
        let grow = if kill { 1.5 - 0.38 * t.min(1.0) } else { 1.38 - 0.38 * (t * 3.0).min(1.0) };
        let side = 30.0 * k * grow;
        let colour = with_alpha(if kill { HIT_RED } else { WHITE }, 1.0 - t * t);
        s.image("damage_feedback", w / 2.0 - side / 2.0, h / 2.0 - side / 2.0, side, side, colour);
    }
}

static HUD: Mutex<Option<Hud>> = Mutex::new(None);

fn with_hud<R>(f: impl FnOnce(&mut Hud) -> R) -> R {
    let mut g = HUD.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Hud::default))
}

/// Takes one value for the HUD (see the module's table). Strings are borrowed
/// for the call; a null one is ignored.
#[no_mangle]
pub extern "C" fn codlegacy_ui_set(key: *const c_char, value: *const c_char) {
    if key.is_null() || value.is_null() {
        return;
    }
    // SAFETY: both are non-null NUL-terminated strings the caller keeps alive
    // for the call.
    let (key, value) = unsafe { (CStr::from_ptr(key).to_string_lossy(), CStr::from_ptr(value).to_string_lossy()) };
    with_hud(|h| h.set(&key, &value, Instant::now()));
}

/// Draws the HUD for one frame of a `width` x `height` screen with the
/// engine's drawing functions in `api` (null: nothing). Returns whether the
/// HUD was up and drawn.
#[no_mangle]
pub extern "C" fn codlegacy_ui_frame(api: *const UiApi, width: f32, height: f32) -> bool {
    // SAFETY: a non-null `api` points to a table that lives for the call.
    let Some(api) = (unsafe { api.as_ref() }) else { return false };
    if !(width > 1.0 && height > 1.0) {
        return false;
    }
    let now = Instant::now();
    with_hud(|h| {
        let up = h.up(now);
        h.draw(&mut Engine(api), width, height, now);
        up
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder {
        fills: Vec<(f32, f32, f32, f32)>,
        texts: Vec<String>,
        images: Vec<String>,
    }

    impl Surface for Recorder {
        fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, _: [f32; 4]) {
            self.fills.push((x, y, w, h));
        }
        fn text(&mut self, s: &str, _: f32, _: f32, _: i32, _: [f32; 4]) {
            self.texts.push(s.to_owned());
        }
        fn width(&mut self, s: &str, size: i32) -> f32 {
            s.len() as f32 * size as f32 * 0.5
        }
        fn image(&mut self, m: &str, _: f32, _: f32, _: f32, _: f32, _: [f32; 4]) {
            self.images.push(m.to_owned());
        }
    }

    fn up_hud(now: Instant) -> Hud {
        let mut h = Hud::default();
        h.set("beat", "1", now);
        h
    }

    #[test]
    fn nothing_is_drawn_until_the_hud_is_up_or_after_it_goes_stale() {
        let t0 = Instant::now();
        let mut r = Recorder::default();
        let mut h = Hud::default();
        h.draw(&mut r, 1920.0, 1080.0, t0);
        assert!(r.fills.is_empty());
        h.set("beat", "1", t0);
        h.draw(&mut r, 1920.0, 1080.0, t0);
        assert!(!r.fills.is_empty());
        let mut stale = Recorder::default();
        h.draw(&mut stale, 1920.0, 1080.0, t0 + Duration::from_secs(2));
        assert!(stale.fills.is_empty() && stale.texts.is_empty());
    }

    #[test]
    fn scores_clock_gun_and_name_are_written() {
        let t0 = Instant::now();
        let mut h = up_hud(t0);
        for (k, v) in [
            ("weapon", "MCW"),
            ("clip", "27"),
            ("stock", "90"),
            ("health", "100"),
            ("name", "sero"),
            ("mine", "12"),
            ("theirs", "9"),
            ("mine_name", "ALLIES"),
            ("theirs_name", "AXIS"),
            ("limit", "75"),
            ("clock", "125000"),
        ] {
            h.set(k, v, t0);
        }
        let mut r = Recorder::default();
        h.draw(&mut r, 1920.0, 1080.0, t0);
        for want in ["MCW", "27", "90", "sero", "12", "9", "ALLIES", "AXIS", "2:05", "WINNING"] {
            assert!(r.texts.iter().any(|t| t == want), "missing {want}: {:?}", r.texts);
        }
    }

    #[test]
    fn the_score_bar_fills_by_the_limit() {
        let t0 = Instant::now();
        let mut h = up_hud(t0);
        h.set("mine", "50", t0);
        h.set("limit", "100", t0);
        let mut r = Recorder::default();
        h.draw(&mut r, 1920.0, 1080.0, t0);
        // (the 260-wide bar, half full, on a 1080-line screen)
        assert!(r.fills.iter().any(|f| (f.2 - 130.0).abs() < 1.0 && (f.3 - 12.0).abs() < 0.1), "{:?}", r.fills);
    }

    #[test]
    fn the_feed_keeps_five_rows_and_fades_them_out() {
        let t0 = Instant::now();
        let mut h = up_hud(t0);
        for i in 0..7 {
            h.set("feed", &format!("T:ffcc4d:a{i}|I:killicon|T:e64538:v{i}"), t0);
        }
        assert_eq!(h.feed.len(), 5);
        let mut r = Recorder::default();
        h.draw(&mut r, 1920.0, 1080.0, t0);
        assert_eq!(r.images.len(), 5);
        assert!(r.texts.iter().any(|t| t == "a6") && !r.texts.iter().any(|t| t == "a1"));
        // Past its life it is gone.
        h.set("beat", "1", t0 + Duration::from_secs(7));
        let mut gone = Recorder::default();
        h.draw(&mut gone, 1920.0, 1080.0, t0 + Duration::from_secs(7));
        assert!(gone.images.is_empty());
    }

    #[test]
    fn a_hit_marker_shows_then_goes() {
        let t0 = Instant::now();
        let mut h = up_hud(t0);
        h.set("hit", "kill", t0);
        let mut r = Recorder::default();
        h.draw(&mut r, 1920.0, 1080.0, t0 + Duration::from_millis(50));
        assert_eq!(r.images, vec!["damage_feedback"]);
        h.set("beat", "1", t0 + Duration::from_millis(900));
        let mut later = Recorder::default();
        h.draw(&mut later, 1920.0, 1080.0, t0 + Duration::from_millis(900));
        assert!(later.images.is_empty());
        // A hit just after a kill does not replace the kill's X.
        let mut h = up_hud(t0);
        h.set("hit", "kill", t0);
        h.set("hit", "hit", t0 + Duration::from_millis(50));
        assert!(matches!(h.hit, Some((_, true))));
    }

    #[test]
    fn feed_text_parses_with_colons_in_the_text() {
        let parts = parse_feed("T:ffffff:a:b|I:icon|X:junk|T:zz:");
        assert_eq!(parts, vec![Part::Text("a:b".into(), [1.0, 1.0, 1.0, 1.0]), Part::Image("icon".into())]);
    }
}
