//! The sound device: rodio on the default output. One [`Player`] per playing
//! voice; the director sets each voice's volume every tick.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Source};

use super::Backend;

pub struct RodioBackend {
    // Dropping the sink closes the device, so it lives as long as the voices.
    sink: MixerDeviceSink,
    voices: HashMap<u64, rodio::Player>,
}

impl RodioBackend {
    /// Opens the default output device.
    pub fn open() -> Result<Self, String> {
        let mut sink = DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
        sink.log_on_drop(false);
        Ok(Self { sink, voices: HashMap::new() })
    }

    fn decoder(path: &Path) -> Result<Decoder<std::io::BufReader<File>>, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        Decoder::try_from(file).map_err(|e| e.to_string())
    }
}

impl Backend for RodioBackend {
    fn prepare(&mut self, path: &Path) -> Result<Option<f64>, String> {
        let decoder = Self::decoder(path)?;
        Ok(decoder.total_duration().map(|d| d.as_secs_f64()))
    }

    fn start(&mut self, voice: u64, path: &Path) -> Result<(), String> {
        let decoder = Self::decoder(path)?;
        let player = rodio::Player::connect_new(self.sink.mixer());
        player.set_volume(0.0);
        player.append(decoder);
        self.voices.insert(voice, player);
        Ok(())
    }

    fn set_gain(&mut self, voice: u64, gain: f32) {
        if let Some(p) = self.voices.get(&voice) {
            p.set_volume(gain.clamp(0.0, 1.0));
        }
    }

    fn stop(&mut self, voice: u64) {
        if let Some(p) = self.voices.remove(&voice) {
            p.stop();
        }
    }

    fn finished(&mut self) -> Vec<u64> {
        let done: Vec<u64> = self.voices.iter().filter(|(_, p)| p.empty()).map(|(id, _)| *id).collect();
        for id in &done {
            self.voices.remove(id);
        }
        done
    }
}
