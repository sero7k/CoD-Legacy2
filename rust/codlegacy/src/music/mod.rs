//! CoD Legacy's music: a director that plays the user's own music folders
//! (`music/menu`, `music/lobby`, `music/match`) and crossfades between tracks
//! and contexts. The decisions are [`logic::Core`] (ported from the BO2
//! rebuild's music director); this module turns its actions into playback on a
//! [`Backend`], on a thread of its own so a slow file or device never stalls a
//! frame.
//!
//! The client tells it where the game is with [`codlegacy_music_context`] and
//! how loud with [`codlegacy_music_volume`]; nothing else crosses the boundary.

mod config;
pub mod logic;
mod player;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::host;
use config::MusicConfig;
use logic::{Action, Core, MusicContext, Playlist, TrackId};

const LOG: &str = "music";
const TICK: Duration = Duration::from_millis(100);

/// What plays the sound. A real one is [`player::RodioBackend`]; tests use a
/// recorder.
pub trait Backend {
    /// Opens `path` and reports its length when known.
    fn prepare(&mut self, path: &Path) -> Result<Option<f64>, String>;
    /// Starts `path` as `voice`, silent (its gain follows).
    fn start(&mut self, voice: u64, path: &Path) -> Result<(), String>;
    fn set_gain(&mut self, voice: u64, gain: f32);
    fn stop(&mut self, voice: u64);
    /// Voices whose track ran out.
    fn finished(&mut self) -> Vec<u64>;
}

/// The director: the core, the track table and the backend.
pub struct Director<B: Backend> {
    core: Core,
    tracks: Vec<PathBuf>,
    levels: [f32; 3],
    volume: f32,
    backend: B,
    lines: Vec<String>,
}

fn build_tracks(cfg: &MusicConfig, music_dir: &Path) -> (Vec<PathBuf>, [Playlist; 3]) {
    let mut tracks: Vec<PathBuf> = Vec::new();
    let mut ids: HashMap<PathBuf, TrackId> = HashMap::new();
    let mut lists: Vec<Playlist> = Vec::new();
    for (slot, context) in MusicContext::PLAYING.iter().enumerate() {
        let defaults = [config::Entry::Defaults];
        let entries = cfg.playlists[slot].as_deref().unwrap_or(&defaults);
        let mut list = Vec::new();
        for path in config::resolve(*context, entries, music_dir) {
            let id = *ids.entry(path.clone()).or_insert_with(|| {
                tracks.push(path.clone());
                tracks.len() - 1
            });
            list.push(id);
        }
        lists.push(Playlist::new(list, cfg.shuffle));
    }
    let [a, b, c] = <[Playlist; 3]>::try_from(lists).unwrap_or_else(|_| unreachable!("three contexts"));
    (tracks, [a, b, c])
}

impl<B: Backend> Director<B> {
    pub fn new(cfg: &MusicConfig, music_dir: &Path, backend: B, seed: u64) -> Self {
        let (tracks, playlists) = build_tracks(cfg, music_dir);
        Self {
            core: Core::new(cfg.timings, playlists, seed),
            tracks,
            levels: cfg.levels,
            volume: 1.0,
            backend,
            lines: Vec::new(),
        }
    }

    pub fn set_context(&mut self, context: MusicContext, now: f64) {
        self.core.set_context(context, now);
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    /// Lines the director wants printed (drained by the caller).
    pub fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    fn path(&self, track: TrackId) -> Option<&Path> {
        self.tracks.get(track).map(PathBuf::as_path)
    }

    fn name(&self, track: TrackId) -> String {
        self.path(track)
            .and_then(|p| p.file_name())
            .map_or_else(|| format!("#{track}"), |n| n.to_string_lossy().into_owned())
    }

    /// One step at `now` seconds: carries out the core's actions and sets
    /// every voice's volume.
    pub fn step(&mut self, now: f64) {
        for action in self.core.tick(now) {
            match action {
                Action::Prepare { track, .. } => {
                    let path = self.path(track).map(Path::to_path_buf);
                    let result = match path {
                        Some(p) => self.backend.prepare(&p),
                        None => Err("no such track".into()),
                    };
                    match result {
                        Ok(length) => {
                            self.core.ready(track, length);
                        }
                        Err(e) => {
                            self.lines.push(format!("cannot play {}: {e}", self.name(track)));
                            self.core.failed(track);
                        }
                    }
                }
                Action::Start { voice, track, context, .. } => {
                    let path = self.path(track).map(Path::to_path_buf);
                    let started = path.map(|p| self.backend.start(voice, &p));
                    match started {
                        Some(Ok(())) => self.lines.push(format!("{}: {}", context.label(), self.name(track))),
                        other => {
                            let why = match other {
                                Some(Err(e)) => e,
                                _ => "no such track".into(),
                            };
                            self.lines.push(format!("cannot play {}: {why}", self.name(track)));
                            self.core.voice_ended(voice);
                            self.core.failed(track);
                        }
                    }
                }
                Action::Stop { voice, .. } => self.backend.stop(voice),
                Action::TimedOut { track } => self.lines.push(format!("{} took too long to open", self.name(track))),
                Action::Empty { context } => {
                    self.lines.push(format!("nothing to play for {} (put music in music/{})", context.label(), context.label()));
                }
            }
        }
        for voice in self.core.voices() {
            let level = voice.context.slot().map_or(0.0, |s| self.levels[s]);
            let gain = self.core.gain(voice.id, now) * level * self.volume;
            self.backend.set_gain(voice.id, gain);
        }
        for ended in self.backend.finished() {
            self.core.voice_ended(ended);
        }
    }

    /// Everything stops at once.
    pub fn stop_all(&mut self) {
        for action in self.core.stop_all() {
            if let Action::Stop { voice, .. } = action {
                self.backend.stop(voice);
            }
        }
    }
}

enum Msg {
    Context(MusicContext),
    Volume(f32),
    Stop,
}

fn run(rx: mpsc::Receiver<Msg>) {
    let (music_dir, cfg_path) = config::locate();
    let cfg = match &cfg_path {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(text) => config::parse(&text, p.parent().unwrap_or(Path::new("."))),
            Err(e) => {
                host::log(LOG, &format!("cannot read {}: {e}", p.display()));
                MusicConfig::default()
            }
        },
        None => MusicConfig::default(),
    };
    for w in &cfg.warnings {
        host::log(LOG, &format!("music.cfg: {w}"));
    }
    if !cfg.enabled {
        host::log(LOG, "off (enabled = 0)");
        return;
    }
    let backend = match player::RodioBackend::open() {
        Ok(b) => b,
        Err(e) => {
            host::log(LOG, &format!("no sound device: {e}"));
            return;
        }
    };
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    let mut director = Director::new(&cfg, &music_dir, backend, seed);
    host::log(LOG, &format!("music folder {} ({} tracks)", music_dir.display(), director.tracks.len()));
    let start = Instant::now();
    loop {
        match rx.recv_timeout(TICK) {
            Ok(Msg::Context(c)) => director.set_context(c, start.elapsed().as_secs_f64()),
            Ok(Msg::Volume(v)) => director.set_volume(v),
            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => {
                director.stop_all();
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        director.step(start.elapsed().as_secs_f64());
        for line in director.take_lines() {
            host::log(LOG, &line);
        }
    }
}

static TX: Mutex<Option<Sender<Msg>>> = Mutex::new(None);

fn send(msg: Msg) {
    let mut guard = TX.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let (tx, rx) = mpsc::channel();
        if std::thread::Builder::new().name("codlegacy-music".into()).spawn(move || run(rx)).is_err() {
            return;
        }
        *guard = Some(tx);
    }
    if let Some(tx) = guard.as_ref() {
        let _ = tx.send(msg);
    }
}

/// Tells the director what the game is doing: 0 silent (loading), 1 menu,
/// 2 lobby, 3 match. The first call starts the music thread.
#[unsafe(no_mangle)]
pub extern "C" fn codlegacy_music_context(context: i32) {
    let c = match context {
        1 => MusicContext::Menu,
        2 => MusicContext::Lobby,
        3 => MusicContext::Match,
        _ => MusicContext::Silent,
    };
    send(Msg::Context(c));
}

/// The music volume, 0..1 (under each context's own level).
#[unsafe(no_mangle)]
pub extern "C" fn codlegacy_music_volume(volume: f32) {
    send(Msg::Volume(if volume.is_finite() { volume } else { 0.0 }));
}

/// Stops the music and its thread (the game is closing).
#[unsafe(no_mangle)]
pub extern "C" fn codlegacy_music_stop() {
    if let Some(tx) = TX.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = tx.send(Msg::Stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Log {
        started: Vec<(u64, PathBuf)>,
        stopped: Vec<u64>,
        gains: HashMap<u64, f32>,
        ended: Vec<u64>,
    }

    struct Fake(Rc<RefCell<Log>>);

    impl Backend for Fake {
        fn prepare(&mut self, path: &Path) -> Result<Option<f64>, String> {
            if path.to_string_lossy().contains("broken") {
                Err("bad file".into())
            } else {
                Ok(Some(60.0))
            }
        }
        fn start(&mut self, voice: u64, path: &Path) -> Result<(), String> {
            self.0.borrow_mut().started.push((voice, path.to_path_buf()));
            Ok(())
        }
        fn set_gain(&mut self, voice: u64, gain: f32) {
            self.0.borrow_mut().gains.insert(voice, gain);
        }
        fn stop(&mut self, voice: u64) {
            self.0.borrow_mut().stopped.push(voice);
        }
        fn finished(&mut self) -> Vec<u64> {
            std::mem::take(&mut self.0.borrow_mut().ended)
        }
    }

    fn setup(files: &[(&str, &str)]) -> (PathBuf, Director<Fake>, Rc<RefCell<Log>>) {
        setup_with(files, true)
    }

    fn setup_with(files: &[(&str, &str)], shuffle: bool) -> (PathBuf, Director<Fake>, Rc<RefCell<Log>>) {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "codlegacy_dir_{}_{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        for (ctx, name) in files {
            std::fs::create_dir_all(dir.join(ctx)).unwrap();
            std::fs::write(dir.join(ctx).join(name), b"x").unwrap();
        }
        let log = Rc::new(RefCell::new(Log::default()));
        let cfg = MusicConfig { shuffle, ..MusicConfig::default() };
        let d = Director::new(&cfg, &dir, Fake(log.clone()), 7);
        (dir, d, log)
    }

    #[test]
    fn menu_music_starts_and_fades_in_under_the_level_and_volume() {
        let (dir, mut d, log) = setup(&[("menu", "a.mp3")]);
        d.set_volume(0.5);
        d.set_context(MusicContext::Menu, 0.0);
        d.step(0.1);
        d.step(0.2);
        assert_eq!(log.borrow().started.len(), 1);
        d.step(1.0);
        let g = *log.borrow().gains.values().next().unwrap();
        assert!(g > 0.0 && g <= 0.5, "{g}");
        d.step(5.0);
        let g = *log.borrow().gains.values().next().unwrap();
        assert!((g - 0.5).abs() < 1e-4, "{g}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_match_plays_quietly_and_leaving_the_menu_stops_its_voice() {
        let (dir, mut d, log) = setup(&[("menu", "a.mp3"), ("match", "m.ogg")]);
        d.set_context(MusicContext::Menu, 0.0);
        d.step(0.1);
        d.step(0.2);
        d.step(3.0);
        d.set_context(MusicContext::Match, 3.1);
        d.step(3.2);
        d.step(3.3);
        d.step(10.0);
        let l = log.borrow();
        assert!(l.stopped.contains(&1), "{:?}", l.stopped);
        let m = l.started.iter().find(|(_, p)| p.ends_with("m.ogg")).expect("match track started");
        assert!(l.gains[&m.0] <= 0.25 + 1e-4);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_broken_file_is_skipped_and_reported() {
        let (dir, mut d, log) = setup_with(&[("menu", "broken.mp3"), ("menu", "fine.mp3")], false);
        d.set_context(MusicContext::Menu, 0.0);
        for t in 0..40 {
            d.step(t as f64 * 0.1);
        }
        assert!(log.borrow().started.iter().all(|(_, p)| p.ends_with("fine.mp3")));
        assert!(!log.borrow().started.is_empty());
        assert!(d.take_lines().iter().any(|l| l.contains("broken.mp3")));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn an_empty_folder_says_so_once() {
        let (dir, mut d, _log) = setup(&[("lobby", "x.txt")]);
        d.set_context(MusicContext::Lobby, 0.0);
        d.step(0.1);
        d.step(0.2);
        let lines = d.take_lines();
        assert_eq!(lines.iter().filter(|l| l.contains("nothing to play for lobby")).count(), 1, "{lines:?}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_voice_that_runs_dry_is_dropped_and_the_next_track_follows() {
        let (dir, mut d, log) = setup(&[("menu", "a.mp3"), ("menu", "b.mp3")]);
        d.set_context(MusicContext::Menu, 0.0);
        d.step(0.1);
        d.step(0.2);
        assert_eq!(log.borrow().started.len(), 1);
        log.borrow_mut().ended.push(1);
        for t in 3..8 {
            d.step(t as f64 * 0.1);
        }
        assert!(log.borrow().started.len() >= 2, "{:?}", log.borrow().started);
        std::fs::remove_dir_all(dir).ok();
    }
}
