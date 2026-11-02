//! Síntese de áudio simples (oscilador senoidal + envelope) via cpal.
//!
//! Primeiro corte do áudio: Rust puro, sem SoundFont. O relógio do app envia
//! comandos `NoteOn`/`NoteOff` por um canal; o callback de áudio (em sua própria
//! thread) soma as vozes ativas e escreve as amostras na placa de som.
//!
//! O stream é reconstruído ao trocar o dispositivo de saída (ver [`SynthRuntime`]).

use std::sync::mpsc::{channel, Receiver, Sender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Comandos enviados do relógio (thread da UI) para o sintetizador (thread de áudio).
pub enum SynthCmd {
    NoteOn(u8),
    NoteOff(u8),
    AllOff,
}

/// Stream de áudio ativo + canal de comandos. Trocar de dispositivo substitui tudo.
pub struct SynthRuntime {
    _stream: Option<cpal::Stream>,
    pub tx: Option<Sender<SynthCmd>>,
}

impl SynthRuntime {
    /// Runtime silencioso (sem áudio) — usado em falhas.
    pub fn silent() -> Self {
        SynthRuntime {
            _stream: None,
            tx: None,
        }
    }

    /// Inicia o áudio no dispositivo de saída padrão.
    pub fn default_output() -> Self {
        match cpal::default_host().default_output_device() {
            Some(device) => Self::on_device(&device),
            None => Self::silent(),
        }
    }

    /// Inicia o áudio num dispositivo específico. Em falha, retorna silencioso.
    pub fn on_device(device: &cpal::Device) -> Self {
        let Ok(supported) = device.default_output_config() else {
            return Self::silent();
        };
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.config();
        let sr = config.sample_rate.0 as f32;
        let channels = config.channels as usize;

        let (tx, rx) = channel::<SynthCmd>();
        let synth = Synth::new(sr);

        let result = match sample_format {
            cpal::SampleFormat::F32 => run::<f32>(device, &config, rx, synth, channels),
            cpal::SampleFormat::I16 => run::<i16>(device, &config, rx, synth, channels),
            cpal::SampleFormat::U16 => run::<u16>(device, &config, rx, synth, channels),
            _ => return Self::silent(),
        };

        let Ok(stream) = result else {
            return Self::silent();
        };
        if stream.play().is_err() {
            return Self::silent();
        }
        SynthRuntime {
            _stream: Some(stream),
            tx: Some(tx),
        }
    }
}

/// Nomes dos dispositivos de saída disponíveis.
pub fn list_output_devices() -> Vec<String> {
    match cpal::default_host().output_devices() {
        Ok(devs) => devs.filter_map(|d| d.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Nome do dispositivo de saída padrão.
pub fn default_output_name() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .and_then(|d| d.name().ok())
}

/// Procura um dispositivo de saída pelo nome.
pub fn device_by_name(name: &str) -> Option<cpal::Device> {
    cpal::default_host()
        .output_devices()
        .ok()?
        .find(|d| d.name().map(|n| n == name).unwrap_or(false))
}

struct Voice {
    key: u8,
    freq: f32,
    phase: f32,
    env: f32,
    releasing: bool,
}

struct Synth {
    sr: f32,
    atk: f32, // incremento de envelope por amostra no ataque
    rel: f32, // decremento por amostra no release
    // Lista de vozes: notas repetidas coexistem (a antiga faz release enquanto a nova ataca),
    // evitando o clique que havia ao substituir a voz na marra.
    voices: Vec<Voice>,
}

impl Synth {
    fn new(sr: f32) -> Self {
        Synth {
            sr,
            atk: 1.0 / (0.008 * sr), // ~8ms de ataque
            rel: 1.0 / (0.15 * sr),  // ~150ms de release (fade out suave)
            voices: Vec::new(),
        }
    }

    fn handle(&mut self, cmd: SynthCmd) {
        match cmd {
            SynthCmd::NoteOn(key) => {
                self.voices.push(Voice {
                    key,
                    freq: midi_to_freq(key),
                    phase: 0.0,
                    env: 0.0,
                    releasing: false,
                });
            }
            SynthCmd::NoteOff(key) => {
                // solta a voz mais recente ainda ativa daquela tecla
                if let Some(v) = self
                    .voices
                    .iter_mut()
                    .rev()
                    .find(|v| v.key == key && !v.releasing)
                {
                    v.releasing = true;
                }
            }
            SynthCmd::AllOff => {
                for v in self.voices.iter_mut() {
                    v.releasing = true;
                }
            }
        }
    }

    /// Próxima amostra mono (soma das vozes).
    fn next_sample(&mut self) -> f32 {
        let (atk, rel, sr) = (self.atk, self.rel, self.sr);
        let mut mix = 0.0f32;
        for v in self.voices.iter_mut() {
            if v.releasing {
                v.env -= rel;
            } else {
                v.env += atk;
                if v.env > 1.0 {
                    v.env = 1.0;
                }
            }
            if v.env < 0.0 {
                v.env = 0.0;
            }
            mix += (v.phase * std::f32::consts::TAU).sin() * v.env * 0.15;
            v.phase += v.freq / sr;
            if v.phase >= 1.0 {
                v.phase -= 1.0;
            }
        }
        // remove vozes que terminaram o release
        self.voices.retain(|v| !(v.releasing && v.env <= 0.0));
        mix.clamp(-1.0, 1.0)
    }
}

fn midi_to_freq(key: u8) -> f32 {
    440.0 * 2f32.powf((key as f32 - 69.0) / 12.0)
}

fn run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    rx: Receiver<SynthCmd>,
    mut synth: Synth,
    channels: usize,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            for cmd in rx.try_iter() {
                synth.handle(cmd);
            }
            for frame in data.chunks_mut(channels) {
                let sample = T::from_sample(synth.next_sample());
                for out in frame.iter_mut() {
                    *out = sample;
                }
            }
        },
        |e| eprintln!("erro no stream de áudio: {e}"),
        None,
    )
}
