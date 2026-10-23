//! The Legacy menu: a panel over the game in the BO2 rebuild's front-end look
//! (near-black navy panels, one blue accent, white text that dims when it is
//! not in focus). It lists every map in `hmw-usermaps` to launch, and holds
//! the settings MWR asks for (controls, graphics, audio, gameplay). It does not replace H1's own
//! menus: `legacy_menu` opens it from the console or a bind, and while it is
//! open it takes the arrow keys, Enter and Escape.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::host;
use crate::settings::{Setting, CATEGORIES};
use crate::ui::{Engine, Surface, UiApi};

// The rebuild's shell palette (crates/ui/src/shell/theme.rs).
const ACCENT: [f32; 4] = [0.26, 0.62, 0.95, 1.0];
const ACCENT_SOFT: [f32; 4] = [0.26, 0.62, 0.95, 0.30];
const TEXT: [f32; 4] = [0.92, 0.94, 0.97, 0.92];
const DIM: [f32; 4] = [0.80, 0.84, 0.90, 0.58];
const FAINT: [f32; 4] = [0.80, 0.84, 0.90, 0.32];
const PANEL: [f32; 4] = [0.025, 0.04, 0.07, 0.82];
const LINE: [f32; 4] = [0.70, 0.80, 0.95, 0.14];

const VISIBLE_ROWS: usize = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Back,
}

/// An engine key code as a menu key (arrows, the d-pad, the wheel, Enter,
/// Escape, Backspace).
pub fn key_from_engine(code: i32) -> Option<Key> {
    Some(match code {
        0x9A | 0x14 | 0xCE => Key::Up,
        0x9B | 0x15 | 0xCD => Key::Down,
        0x9C | 0x16 => Key::Left,
        0x9D | 0x17 => Key::Right,
        0x0D | 0x01 => Key::Enter,
        0x1B | 0x08 | 0x02 => Key::Back,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Main,
    Maps,
    Options,
    Category(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapEntry {
    pub id: String,
    pub name: String,
}

/// "mp_trailerpark" -> "Trailerpark"; used when an arena has no plain name.
fn name_from_id(id: &str) -> String {
    let base = id.strip_prefix("mp_").unwrap_or(id);
    base.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The `longname` of an arena file's text, when it is a plain name (not a
/// localized reference like `MPUI_BIGBEN2`).
fn arena_name(text: &str) -> Option<String> {
    let line = text.lines().find(|l| l.trim_start().starts_with("longname"))?;
    let value = line.split('"').nth(1)?.trim();
    (!value.is_empty() && !value.starts_with("MPUI_")).then(|| value.to_owned())
}

/// Every folder of `dir` that holds a fastfile of its own name, as a map,
/// sorted by name.
pub fn scan_maps(dir: &Path) -> Vec<MapEntry> {
    let mut maps: Vec<MapEntry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return None;
            }
            let folder = e.path();
            if !folder.join(format!("{id}.ff")).is_file() {
                return None;
            }
            let name = std::fs::read_to_string(folder.join(format!("{id}.arena")))
                .ok()
                .and_then(|t| arena_name(&t))
                .unwrap_or_else(|| name_from_id(&id));
            Some(MapEntry { id, name })
        })
        .collect();
    maps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    maps
}

fn usermaps_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("LEGACY_USERMAPS_DIR") {
        return PathBuf::from(p);
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("hmw-usermaps")))
        .unwrap_or_else(|| PathBuf::from("hmw-usermaps"))
}

pub struct Menu {
    open: bool,
    screen: Screen,
    cursor: usize,
    maps: Vec<MapEntry>,
    values: Vec<String>,
    /// Cursor to return to on the category list.
    category_cursor: usize,
}

impl Default for Menu {
    fn default() -> Self {
        Self { open: false, screen: Screen::Main, cursor: 0, maps: Vec::new(), values: Vec::new(), category_cursor: 0 }
    }
}

const MAIN_ROWS: [&str; 3] = ["MAPS", "OPTIONS", "CLOSE"];

impl Menu {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.screen = Screen::Main;
            self.cursor = 0;
        }
    }

    fn rows(&self) -> usize {
        match self.screen {
            Screen::Main => MAIN_ROWS.len(),
            Screen::Maps => self.maps.len(),
            Screen::Options => CATEGORIES.len(),
            Screen::Category(c) => CATEGORIES[c].settings.len(),
        }
    }

    fn go(&mut self, screen: Screen) {
        self.screen = screen;
        self.cursor = 0;
        match screen {
            Screen::Maps => self.maps = scan_maps(&usermaps_dir()),
            Screen::Category(c) => {
                self.values = CATEGORIES[c]
                    .settings
                    .iter()
                    .map(|st| host::dvar(st.dvar).unwrap_or_default())
                    .collect();
            }
            Screen::Options => {
                self.cursor = self.category_cursor.min(CATEGORIES.len() - 1);
            }
            Screen::Main => {}
        }
    }

    fn change(&mut self, dir: i32) {
        let Screen::Category(c) = self.screen else { return };
        let Some(st) = CATEGORIES[c].settings.get(self.cursor) else { return };
        let now = self.values.get(self.cursor).map(String::as_str).unwrap_or("");
        let next = st.step(now, dir);
        host::exec(&format!("seta {} {}", st.dvar, next));
        if let Some(v) = self.values.get_mut(self.cursor) {
            *v = next;
        }
    }

    /// Handles a key; returns whether the menu took it (always, while open).
    pub fn key(&mut self, key: Key) -> bool {
        if !self.open {
            return false;
        }
        let n = self.rows();
        match key {
            Key::Up if n > 0 => self.cursor = (self.cursor + n - 1) % n,
            Key::Down if n > 0 => self.cursor = (self.cursor + 1) % n,
            Key::Back => match self.screen {
                Screen::Main => self.open = false,
                Screen::Category(_) => self.go(Screen::Options),
                Screen::Category(_) => self.go(Screen::Options),
                _ => self.go(Screen::Main),
            },
            Key::Left | Key::Right if matches!(self.screen, Screen::Category(_)) => {
                self.change(if key == Key::Left { -1 } else { 1 });
            }
            Key::Enter => match (self.screen, self.cursor) {
                (Screen::Main, 0) => self.go(Screen::Maps),
                (Screen::Main, 1) => self.go(Screen::Options),
                (Screen::Main, _) => self.open = false,
                (Screen::Maps, i) => {
                    if let Some(m) = self.maps.get(i) {
                        host::exec(&format!("map {}", m.id));
                        self.open = false;
                    }
                }
                (Screen::Options, i) => {
                    self.category_cursor = i;
                    self.go(Screen::Category(i));
                }
                (Screen::Category(_), _) => self.change(1),
            },
            _ => {}
        }
        true
    }

    pub fn draw(&self, s: &mut dyn Surface, w: f32, h: f32) {
        if !self.open {
            return;
        }
        let k = h / 1080.0;
        let (px, py, pw, ph) = (110.0 * k, 90.0 * k, 600.0 * k, h - 180.0 * k);
        s.fill(px, py, pw, ph, PANEL);
        s.fill(px, py, 4.0 * k, ph, ACCENT);
        s.fill(px, py + 118.0 * k, pw, k.max(1.0), LINE);
        let _ = w;

        let title = match self.screen {
            Screen::Main => "COD LEGACY",
            Screen::Maps => "MAPS",
            Screen::Options => "OPTIONS",
            Screen::Category(c) => CATEGORIES[c].name,
        };
        s.text(title, px + 34.0 * k, py + 70.0 * k, (46.0 * k) as i32, TEXT);
        if self.screen == Screen::Maps {
            let count = format!("{}", self.maps.len());
            let size = (22.0 * k) as i32;
            let cw = s.width(&count, size);
            s.text(&count, px + pw - 30.0 * k - cw, py + 70.0 * k, size, DIM);
        }

        let row_h = 52.0 * k;
        let top = py + 140.0 * k;
        let n = self.rows();
        let first = if n > VISIBLE_ROWS { self.cursor.saturating_sub(VISIBLE_ROWS / 2).min(n - VISIBLE_ROWS) } else { 0 };
        let size = (28.0 * k) as i32;
        for (slot, i) in (first..n.min(first + VISIBLE_ROWS)).enumerate() {
            let y = top + slot as f32 * row_h;
            let on = i == self.cursor;
            if on {
                s.fill(px + 14.0 * k, y, pw - 28.0 * k, row_h - 4.0 * k, ACCENT_SOFT);
                s.fill(px + 14.0 * k, y, 4.0 * k, row_h - 4.0 * k, ACCENT);
            }
            let base = y + row_h * 0.64;
            let colour = if on { TEXT } else { DIM };
            match self.screen {
                Screen::Main => s.text(MAIN_ROWS[i], px + 40.0 * k, base, size, colour),
                Screen::Maps => {
                    let m = &self.maps[i];
                    s.text(&m.name.to_uppercase(), px + 40.0 * k, base, size, colour);
                    let id_size = (18.0 * k) as i32;
                    let iw = s.width(&m.id, id_size);
                    s.text(&m.id, px + pw - 34.0 * k - iw, base - 2.0 * k, id_size, FAINT);
                }
                Screen::Options => s.text(CATEGORIES[i].name, px + 40.0 * k, base, size, colour),
                Screen::Category(c) => {
                    let st: &Setting = &CATEGORIES[c].settings[i];
                    s.text(st.label, px + 40.0 * k, base, size, colour);
                    let cur = self.values.get(i).map(String::as_str).unwrap_or("");
                    let (text, fill) = st.show(cur);
                    let tsize = (22.0 * k) as i32;
                    let mut right = px + pw - 34.0 * k;
                    if let Some(f) = fill {
                        let (bw, bh) = (150.0 * k, 8.0 * k);
                        let by = y + row_h * 0.5 - bh / 2.0;
                        s.fill(right - bw, by, bw, bh, [0.0, 0.0, 0.0, 0.45]);
                        s.fill(right - bw, by, bw * f.clamp(0.0, 1.0), bh, ACCENT);
                        right -= bw + 14.0 * k;
                    }
                    let tw = s.width(&text, tsize);
                    s.text(&text, right - tw, base, tsize, if on { ACCENT } else { colour });
                }
            }
        }
        if self.screen == Screen::Maps && self.maps.is_empty() {
            s.text("NO MAPS IN HMW-USERMAPS", px + 40.0 * k, top + 36.0 * k, size, DIM);
        }
        let hint = match self.screen {
            Screen::Main | Screen::Maps => "UP/DOWN  SELECT      ENTER  CHOOSE      ESC  BACK",
            Screen::Options => "UP/DOWN  SELECT      ENTER  OPEN      ESC  BACK",
            Screen::Category(_) => "LEFT/RIGHT  ADJUST      ESC  BACK",
        };
        s.text(hint, px + 34.0 * k, py + ph - 26.0 * k, (18.0 * k) as i32, FAINT);
    }
}

static MENU: Mutex<Option<Menu>> = Mutex::new(None);

fn with_menu<R>(f: impl FnOnce(&mut Menu) -> R) -> R {
    let mut g = MENU.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Menu::default))
}

/// Opens or closes the menu (the `legacy_menu` command).
#[unsafe(no_mangle)]
pub extern "C" fn codlegacy_menu_toggle() {
    with_menu(Menu::toggle);
}

/// Whether the menu is open (so the client leaves the game's input alone
/// otherwise).
#[unsafe(no_mangle)]
pub extern "C" fn codlegacy_menu_is_open() -> bool {
    with_menu(|m| m.is_open())
}

/// An engine key going down. True when the menu took it.
#[unsafe(no_mangle)]
pub extern "C" fn codlegacy_menu_key(code: i32) -> bool {
    match key_from_engine(code) {
        Some(k) => with_menu(|m| m.key(k)),
        None => with_menu(|m| m.is_open()),
    }
}

/// Draws the menu for one frame (nothing when it is closed).
#[unsafe(no_mangle)]
pub extern "C" fn codlegacy_menu_frame(api: *const UiApi, width: f32, height: f32) {
    // SAFETY: a non-null `api` points to a table that lives for the call.
    let Some(api) = (unsafe { api.as_ref() }) else { return };
    if !(width > 1.0 && height > 1.0) {
        return;
    }
    with_menu(|m| m.draw(&mut Engine(api), width, height));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Rec {
        texts: Vec<String>,
        fills: usize,
    }

    impl Surface for Rec {
        fn fill(&mut self, _: f32, _: f32, _: f32, _: f32, _: [f32; 4]) {
            self.fills += 1;
        }
        fn text(&mut self, s: &str, _: f32, _: f32, _: i32, _: [f32; 4]) {
            self.texts.push(s.to_owned());
        }
        fn width(&mut self, s: &str, size: i32) -> f32 {
            s.len() as f32 * size as f32 * 0.5
        }
        fn image(&mut self, _: &str, _: f32, _: f32, _: f32, _: f32, _: [f32; 4]) {}
    }

    fn maps_dir() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "codlegacy_maps_{}_{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        for (id, arena) in [
            ("mp_rust", Some("{\n  map \"mp_rust\"\n  longname \"Rust\"\n}")),
            ("mp_bigben2", Some("{\n  longname \"MPUI_BIGBEN2\"\n}")),
            ("mp_trailer_park", None),
        ] {
            let d = dir.join(id);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{id}.ff")), b"x").unwrap();
            if let Some(a) = arena {
                std::fs::write(d.join(format!("{id}.arena")), a).unwrap();
            }
        }
        // A folder without its fastfile is not a map.
        std::fs::create_dir_all(dir.join("mp_empty")).unwrap();
        dir
    }

    #[test]
    fn maps_are_found_named_and_sorted() {
        let dir = maps_dir();
        let maps = scan_maps(&dir);
        let got: Vec<(&str, &str)> = maps.iter().map(|m| (m.id.as_str(), m.name.as_str())).collect();
        assert_eq!(got, vec![("mp_bigben2", "Bigben2"), ("mp_rust", "Rust"), ("mp_trailer_park", "Trailer Park")]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn keys_map_from_the_engine() {
        assert_eq!(key_from_engine(0x9A), Some(Key::Up));
        assert_eq!(key_from_engine(0xCD), Some(Key::Down));
        assert_eq!(key_from_engine(0x1B), Some(Key::Back));
        assert_eq!(key_from_engine(0x41), None);
    }

    #[test]
    fn a_closed_menu_takes_no_keys_and_draws_nothing() {
        let mut m = Menu::default();
        assert!(!m.key(Key::Enter));
        let mut r = Rec::default();
        m.draw(&mut r, 1920.0, 1080.0);
        assert!(r.texts.is_empty() && r.fills == 0);
    }

    #[test]
    fn the_cursor_wraps_and_escape_backs_out_then_closes() {
        let mut m = Menu::default();
        m.toggle();
        assert!(m.key(Key::Up));
        assert_eq!(m.cursor, MAIN_ROWS.len() - 1);
        assert!(m.key(Key::Down));
        assert_eq!(m.cursor, 0);
        m.key(Key::Down);
        m.key(Key::Enter);
        assert_eq!(m.screen, Screen::Options);
        m.key(Key::Back);
        assert_eq!(m.screen, Screen::Main);
        m.key(Key::Back);
        assert!(!m.is_open());
    }

    #[test]
    fn the_maps_screen_lists_the_maps_and_draws_them() {
        let dir = maps_dir();
        let mut m = Menu::default();
        m.toggle();
        m.screen = Screen::Maps;
        m.maps = scan_maps(&dir);
        let mut r = Rec::default();
        m.draw(&mut r, 1920.0, 1080.0);
        for want in ["MAPS", "3", "RUST", "mp_rust", "BIGBEN2", "TRAILER PARK"] {
            assert!(r.texts.iter().any(|t| t == want), "missing {want}: {:?}", r.texts);
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_category_lists_its_settings_and_adjusting_changes_the_row() {
        let mut m = Menu::default();
        m.toggle();
        m.screen = Screen::Options;
        m.key(Key::Down); // GRAPHICS
        m.key(Key::Enter);
        assert_eq!(m.screen, Screen::Category(1));
        m.values = vec!["1".into(); CATEGORIES[1].settings.len()];
        m.key(Key::Enter); // fullscreen toggle
        assert_eq!(m.values[0], "0");
        m.key(Key::Down);
        m.key(Key::Left); // vsync
        assert_eq!(m.values[1], "0");
        let mut r = Rec::default();
        m.draw(&mut r, 1920.0, 1080.0);
        for want in ["GRAPHICS", "FULLSCREEN", "VSYNC", "OFF"] {
            assert!(r.texts.iter().any(|t| t == want), "missing {want}: {:?}", r.texts);
        }
        m.key(Key::Back);
        assert_eq!(m.screen, Screen::Options);
        assert_eq!(m.cursor, 1);
    }

    #[test]
    fn a_long_list_scrolls_with_the_cursor() {
        let mut m = Menu::default();
        m.toggle();
        m.screen = Screen::Maps;
        m.maps = (0..40).map(|i| MapEntry { id: format!("mp_m{i:02}"), name: format!("M{i:02}") }).collect();
        m.cursor = 30;
        let mut r = Rec::default();
        m.draw(&mut r, 1920.0, 1080.0);
        assert!(r.texts.iter().any(|t| t == "M30"));
        assert!(!r.texts.iter().any(|t| t == "M00"));
    }
}
