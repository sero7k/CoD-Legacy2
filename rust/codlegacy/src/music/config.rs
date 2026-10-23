//! `music.cfg` and the music folders.
//!
//! Where things are looked for:
//! * the music folder: `LEGACY_MUSIC_DIR`, else `<exe dir>/music`; its
//!   `menu/`, `lobby/` and `match/` subfolders feed those playlists (`.mp3`,
//!   `.ogg`, `.wav`, `.flac`);
//! * the config: `LEGACY_MUSIC_CFG`, else `<exe dir>/music.cfg`, else
//!   `<music folder>/music.cfg`.
//!
//! The config is `key = value` lines (`//` comments, or `#` at a line's start):
//! ```text
//! enabled = 1          // 0: no music at all
//! shuffle = 1          // 0: play each playlist in order
//! menu_level = 1.0     // per-context level, under legacy_music_volume
//! lobby_level = 1.0
//! match_level = 0.25   // in-match ambience stays low
//! fade_in = 2.0        // seconds
//! fade_out = 1.5
//! crossfade = 4.0
//! menu = defaults      // a playlist line; any line for a context replaces
//! menu = D:/Music/x.mp3  // its defaults. An entry is `defaults` (the
//! lobby = music/lobby  // context's folder), a folder, or a file.
//! match = none         // an empty playlist
//! ```

use std::path::{Path, PathBuf};

use super::logic::{FadeTimings, MusicContext};

/// File extensions the player can decode.
pub const MUSIC_EXTENSIONS: [&str; 4] = ["mp3", "ogg", "wav", "flac"];

pub fn is_music_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MUSIC_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// One playlist line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// The context's own folder under the music folder.
    Defaults,
    /// Nothing: an empty playlist.
    None,
    /// A file, or a folder of files.
    Path(PathBuf),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MusicConfig {
    pub enabled: bool,
    pub shuffle: bool,
    pub levels: [f32; 3],
    pub timings: FadeTimings,
    /// Per context (menu, lobby, match): `None` = the defaults.
    pub playlists: [Option<Vec<Entry>>; 3],
    pub warnings: Vec<String>,
}

impl Default for MusicConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            shuffle: true,
            levels: [1.0, 1.0, 0.25],
            timings: FadeTimings::default(),
            playlists: [None, None, None],
            warnings: Vec::new(),
        }
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn parse_entry(value: &str, base: &Path) -> Entry {
    let v = value.trim().trim_matches('"');
    if v.eq_ignore_ascii_case("defaults") {
        Entry::Defaults
    } else if v.eq_ignore_ascii_case("none") || v.is_empty() {
        Entry::None
    } else {
        let p = Path::new(v);
        Entry::Path(if p.is_absolute() { p.to_path_buf() } else { base.join(p) })
    }
}

fn strip_comment(line: &str) -> &str {
    let line = line.trim();
    if line.starts_with('#') {
        return "";
    }
    // `//` starts a comment, except inside a path like `D://x` (a colon
    // directly before it).
    let bytes = line.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'/' && bytes[i + 1] == b'/' && (i == 0 || bytes[i - 1] != b':') {
            return line[..i].trim();
        }
    }
    line
}

/// Parses a config's text; `base` resolves relative paths.
pub fn parse(text: &str, base: &Path) -> MusicConfig {
    let mut cfg = MusicConfig::default();
    for (n, raw) in text.lines().enumerate() {
        let line = strip_comment(raw);
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            cfg.warnings.push(format!("line {}: expected `key = value`", n + 1));
            continue;
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        let float = |name: &str, cfg: &mut MusicConfig| match value.parse::<f64>() {
            Ok(v) if v.is_finite() && v >= 0.0 => Some(v),
            _ => {
                cfg.warnings.push(format!("line {}: `{name}` needs a number >= 0", n + 1));
                None
            }
        };
        match key.as_str() {
            "enabled" | "shuffle" => match parse_bool(value) {
                Some(b) if key == "enabled" => cfg.enabled = b,
                Some(b) => cfg.shuffle = b,
                None => cfg.warnings.push(format!("line {}: `{key}` is 0 or 1", n + 1)),
            },
            "menu_level" | "lobby_level" | "match_level" => {
                if let Some(v) = float(&key, &mut cfg) {
                    let slot = ["menu_level", "lobby_level", "match_level"]
                        .iter()
                        .position(|k| *k == key)
                        .unwrap_or(0);
                    cfg.levels[slot] = (v as f32).min(1.0);
                }
            }
            "fade_in" => cfg.timings.fade_in = float(&key, &mut cfg).unwrap_or(cfg.timings.fade_in),
            "fade_out" => cfg.timings.fade_out = float(&key, &mut cfg).unwrap_or(cfg.timings.fade_out),
            "crossfade" => cfg.timings.crossfade = float(&key, &mut cfg).unwrap_or(cfg.timings.crossfade),
            "menu" | "lobby" | "match" => {
                let slot = ["menu", "lobby", "match"].iter().position(|k| *k == key).unwrap_or(0);
                cfg.playlists[slot]
                    .get_or_insert_with(Vec::new)
                    .push(parse_entry(value, base));
            }
            _ => cfg.warnings.push(format!("line {}: unknown key `{key}`", n + 1)),
        }
    }
    cfg
}

/// The music files of a folder (one level), sorted by name.
pub fn scan_dir(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_music_file(p))
        .collect();
    files.sort();
    files
}

/// The files a playlist's entries name, in order, each once.
pub fn resolve(context: MusicContext, entries: &[Entry], music_dir: &Path) -> Vec<PathBuf> {
    let folder = music_dir.join(context.label());
    let mut out: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    for e in entries {
        match e {
            Entry::None => {}
            Entry::Defaults => scan_dir(&folder).into_iter().for_each(&mut add),
            Entry::Path(p) if p.is_dir() => scan_dir(p).into_iter().for_each(&mut add),
            Entry::Path(p) if is_music_file(p) && p.is_file() => add(p.clone()),
            Entry::Path(_) => {}
        }
    }
    out
}

/// The music folder and the config file (if any), from the environment and
/// the exe's folder.
pub fn locate() -> (PathBuf, Option<PathBuf>) {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let music_dir = std::env::var_os("LEGACY_MUSIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| exe_dir.join("music"));
    let cfg = std::env::var_os("LEGACY_MUSIC_CFG")
        .map(PathBuf::from)
        .into_iter()
        .chain([exe_dir.join("music.cfg"), music_dir.join("music.cfg")])
        .find(|p| p.is_file());
    (music_dir, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_levels_fades_and_playlists_parse() {
        let base = Path::new("/game");
        let cfg = parse(
            "enabled = 1\nshuffle = off // in order\nmatch_level = 0.4\nfade_in = 3\n\
             menu = defaults\nmenu = D://Music/x.mp3\nmatch = none\n# note\nbogus = 1\nno equals\n",
            base,
        );
        assert!(cfg.enabled && !cfg.shuffle);
        assert_eq!(cfg.levels, [1.0, 1.0, 0.4]);
        assert_eq!(cfg.timings.fade_in, 3.0);
        let menu = cfg.playlists[0].as_ref().expect("menu playlist");
        assert_eq!(menu[0], Entry::Defaults);
        assert!(matches!(&menu[1], Entry::Path(p) if p.ends_with("x.mp3")), "{menu:?}");
        assert_eq!(cfg.playlists[1], None);
        assert_eq!(cfg.playlists[2], Some(vec![Entry::None]));
        assert_eq!(cfg.warnings.len(), 2, "{:?}", cfg.warnings);
    }

    #[test]
    fn bad_numbers_keep_the_default_and_warn() {
        let cfg = parse("crossfade = -2\nmenu_level = abc\n", Path::new("."));
        assert_eq!(cfg.timings.crossfade, FadeTimings::default().crossfade);
        assert_eq!(cfg.warnings.len(), 2);
    }

    #[test]
    fn relative_paths_resolve_against_the_base() {
        assert_eq!(parse_entry("music/menu", Path::new("/g")), Entry::Path(PathBuf::from("/g/music/menu")));
        assert_eq!(parse_entry("  none ", Path::new("/g")), Entry::None);
    }

    #[test]
    fn folders_and_files_resolve_once_and_only_music() {
        let dir = std::env::temp_dir().join(format!("codlegacy_music_{}", std::process::id()));
        let menu = dir.join("menu");
        std::fs::create_dir_all(&menu).unwrap();
        for f in ["b.mp3", "a.ogg", "notes.txt"] {
            std::fs::write(menu.join(f), b"x").unwrap();
        }
        let got = resolve(
            MusicContext::Menu,
            &[Entry::Defaults, Entry::Path(menu.clone()), Entry::Path(menu.join("a.ogg"))],
            &dir,
        );
        assert_eq!(got, vec![menu.join("a.ogg"), menu.join("b.mp3")]);
        assert!(resolve(MusicContext::Lobby, &[Entry::Defaults], &dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
