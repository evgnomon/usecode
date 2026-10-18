// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Hold Space at an empty prompt to talk: the microphone records while Space
//! is held, and what was said is sent as the line, transcribed by OpenRouter.
//!
//! A terminal tells a key's press, not its release, so holding is told by the
//! key repeating: a second Space within `HOLD` starts it, and no Space for
//! `RELEASE` ends it. A tap types a space as usual. Recording starts with the
//! first Space, so the first word isn't lost while waiting for the repeat. The
//! microphone is the default source of PulseAudio, or of PipeWire through its
//! pulse server, spoken to natively. The model is `UC_VOICE_MODEL`, the
//! language `UC_VOICE_LANGUAGE`, guessed from the audio when unset.

use super::{client, env};
use anyhow::{Context, Result, bail};
use base64::Engine;
use pulseaudio::protocol;
use rustyline::{Cmd, ConditionalEventHandler, Event, EventContext, RepeatCount};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const API: &str = "https://openrouter.ai/api/v1/audio/transcriptions";
const DEFAULT_MODEL: &str = "openai/gpt-4o-transcribe";
/// How long after the first Space its repeat may come: key repeat starts
/// after 500 to 660 ms on most desktops.
const HOLD: Duration = Duration::from_millis(700);
/// How long without a repeat counts as letting go.
const RELEASE: Duration = Duration::from_millis(250);
/// Shorter than this was a slip, not something said.
const MIN_SPOKEN: Duration = Duration::from_millis(300);
const RATE: u32 = 16_000;
/// Quieter than this all along, out of 32767, is silence: a muted microphone.
const SILENT: u16 = 500;

/// What was said, waiting for `read_line` to return it as the line.
pub static SAID: Mutex<Option<String>> = Mutex::new(None);

/// Bound to Space: on an empty line, a hold records and sends what was said.
pub struct HoldToTalk;

impl ConditionalEventHandler for HoldToTalk {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        if !ctx.line().is_empty() {
            return None;
        }
        let recording = Recording::start();
        match next_key(HOLD) {
            None => return Some(Cmd::SelfInsert(1, ' ')),
            Some(key) if key.iter().all(|&b| b == b' ') => {}
            // A tap, and they went on typing.
            Some(key) => {
                let typed = String::from_utf8(key)
                    .ok()
                    .filter(|t| !t.contains(char::is_control));
                return Some(Cmd::Insert(1, format!(" {}", typed.unwrap_or_default())));
            }
        }
        eprint!(
            "\r\x1b[2K\x1b[31m●\x1b[0m listening, let go of Space to send, any other key to cancel"
        );
        let cancelled = loop {
            match next_key(RELEASE) {
                None => break false,
                Some(key) if key.iter().all(|&b| b == b' ') => {}
                Some(_) => break true,
            }
        };
        let pcm = recording.stop();
        if cancelled {
            eprint!("\r\x1b[2K");
            return Some(Cmd::Repaint);
        }
        eprint!("\r\x1b[2K· transcribing");
        let said = pcm.and_then(transcribe_blocking);
        eprint!("\r\x1b[2K");
        match said {
            Ok(text) if !text.is_empty() => {
                *SAID.lock().unwrap_or_else(|e| e.into_inner()) = Some(text);
                Some(Cmd::AcceptLine)
            }
            Ok(_) => {
                eprint!("· heard nothing\r\n");
                Some(Cmd::Repaint)
            }
            Err(err) => {
                eprint!("· voice: {err:#}\r\n");
                Some(Cmd::Repaint)
            }
        }
    }
}

/// The bytes of the next key pressed within `wait`, read straight off the
/// terminal rustyline has in raw mode.
fn next_key(wait: Duration) -> Option<Vec<u8>> {
    let mut fd = libc::pollfd {
        fd: 0,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one pollfd, and it is valid for the call.
    if unsafe { libc::poll(&mut fd, 1, wait.as_millis() as i32) } <= 0 {
        return None;
    }
    let mut buf = [0u8; 64];
    // SAFETY: reads at most buf.len() bytes into buf.
    let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
    (n > 0).then(|| buf[..n as usize].to_vec())
}

/// The microphone, recorded on a thread of its own until stopped or dropped.
struct Recording {
    stop: tokio::sync::oneshot::Sender<()>,
    thread: std::thread::JoinHandle<Result<Vec<u8>>>,
}

impl Recording {
    fn start() -> Recording {
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let runtime = tokio::runtime::Handle::current();
        let thread = std::thread::spawn(move || runtime.block_on(record(stopped)));
        Recording { stop, thread }
    }

    /// 16 kHz mono 16-bit PCM of what was recorded.
    fn stop(self) -> Result<Vec<u8>> {
        let _ = self.stop.send(());
        self.thread
            .join()
            .map_err(|_| anyhow::anyhow!("the recording thread panicked"))?
    }
}

/// Records the default source until `stopped` is sent or dropped; the server converts it to
/// 16 kHz mono, which is plenty for speech.
async fn record(stopped: tokio::sync::oneshot::Receiver<()>) -> Result<Vec<u8>> {
    let pulse = pulseaudio::Client::from_env(c"usecode")
        .context("no PulseAudio or PipeWire server to record from")?;
    let pcm = Arc::new(Mutex::new(Vec::new()));
    let sink = {
        let pcm = pcm.clone();
        move |data: &[u8]| pcm.lock().unwrap().extend_from_slice(data)
    };
    let params = protocol::RecordStreamParams {
        source_name: Some(protocol::DEFAULT_SOURCE.to_owned()),
        sample_spec: protocol::SampleSpec {
            format: protocol::SampleFormat::S16Le,
            channels: 1,
            sample_rate: RATE,
        },
        channel_map: protocol::ChannelMap::mono(),
        cvolume: Some(protocol::ChannelVolume::norm(1)),
        // 50 ms at a time: the default holds back about two seconds, which
        // are lost when the stream closes.
        buffer_attr: protocol::stream::BufferAttr {
            fragment_size: RATE / 10,
            ..Default::default()
        },
        ..Default::default()
    };
    let stream = pulse
        .create_record_stream(params, sink)
        .await
        .context("opening the microphone")?;
    let _ = stopped.await;
    stream.delete().await.context("closing the microphone")?;
    Ok(std::mem::take(&mut *pcm.lock().unwrap()))
}

/// `transcribe` from the synchronous key handler, which runs inside the
/// runtime and can't wait on it itself.
fn transcribe_blocking(pcm: Vec<u8>) -> Result<String> {
    let runtime = tokio::runtime::Handle::current();
    std::thread::spawn(move || runtime.block_on(transcribe(pcm)))
        .join()
        .map_err(|_| anyhow::anyhow!("the transcription thread panicked"))?
}

/// What was said in the recording, by OpenRouter's speech to text.
async fn transcribe(pcm: Vec<u8>) -> Result<String> {
    let spoken = Duration::from_secs_f64(pcm.len() as f64 / (2 * RATE) as f64);
    // Silence comes back as a made-up word or two.
    if spoken < MIN_SPOKEN || loudest(&pcm) < SILENT {
        return Ok(String::new());
    }
    let key = env("OPENROUTER_API_KEY").context("set OPENROUTER_API_KEY")?;
    let mut body = json!({
        "model": env("UC_VOICE_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        "input_audio": {
            "data": base64::engine::general_purpose::STANDARD.encode(wav(&pcm)),
            "format": "wav",
        },
    });
    if let Some(language) = env("UC_VOICE_LANGUAGE") {
        body["language"] = json!(language);
    }
    let response = client()
        .post(API)
        .bearer_auth(key)
        .header("X-Title", "usecode")
        .json(&body)
        .timeout(Duration::from_secs(90))
        .send()
        .await
        .context("calling OpenRouter")?;
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .context("reading OpenRouter's reply")?;
    if !status.is_success() || value.get("error").is_some() {
        let msg = value["error"]["message"].as_str().unwrap_or("no message");
        bail!("OpenRouter: {status}: {msg}");
    }
    Ok(value["text"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string())
}

/// The loudest sample of 16-bit PCM.
fn loudest(pcm: &[u8]) -> u16 {
    pcm.as_chunks::<2>()
        .0
        .iter()
        .map(|&s| i16::from_le_bytes(s).unsigned_abs())
        .max()
        .unwrap_or(0)
}

/// 16 kHz mono 16-bit PCM in a WAV file.
fn wav(pcm: &[u8]) -> Vec<u8> {
    let len = pcm.len() as u32;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header() {
        let file = wav(&[1, 2, 3, 4]);
        assert_eq!(file.len(), 48);
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(file[4..8].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(file[24..28].try_into().unwrap()), RATE);
        assert_eq!(u32::from_le_bytes(file[40..44].try_into().unwrap()), 4);
        assert_eq!(&file[44..], &[1, 2, 3, 4]);
    }

    #[test]
    fn loudest_sample() {
        assert_eq!(loudest(&[]), 0);
        assert_eq!(loudest(&[0x10, 0x00, 0x00, 0x80]), 32768);
    }
}
