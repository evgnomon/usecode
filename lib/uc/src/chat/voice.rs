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
//! pulse server, spoken to natively.
//!
//! The settings are under `voice:` in `agent.yaml` (see
//! [`crate::providers`]). The model is `model:`, by default MAI-Transcribe 2,
//! which is accurate and cheap and takes a list of words to expect; the
//! language is `language:`, else the one the user's locale is in
//! (auto-detection is poor on a two-second recording); and the repository
//! names under `~/src`, the commands on PATH and `terms:` are sent along as
//! words to expect (`terms: false` turns that off), under every engine's own name for them,
//! so `kubectl` and `usecode` come out right. The reply is asked for word by
//! word, and one of silence or of a word too short to be one is dropped.

use super::{client, env};
use crate::dispatch::search_dirs;
use anyhow::{Context, Result, bail};
use base64::Engine;
use pulseaudio::protocol;
use rustyline::{Cmd, ConditionalEventHandler, Event, EventContext, RepeatCount};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const API: &str = "https://openrouter.ai/api/v1/audio/transcriptions";
/// MAI-Transcribe 2: the best of the engines on a short command, and a tenth
/// of gpt-4o-transcribe's price.
const DEFAULT_MODEL: &str = "microsoft/mai-transcribe-2";
/// How long after the first Space its repeat may come: key repeat starts
/// after 500 to 660 ms on most desktops.
const HOLD: Duration = Duration::from_millis(700);
/// How long without a repeat counts as letting go.
const RELEASE: Duration = Duration::from_millis(250);
/// Shorter than this was a slip, not something said.
const MIN_SPOKEN: Duration = Duration::from_millis(300);
const RATE: u32 = 16_000;
/// What loudness is measured over: 20 ms, about a sound of a word.
const FRAME: usize = RATE as usize / 50;
/// A stretch this many times louder than the quiet of the same recording is
/// a voice. Measured against the recording itself, since a webcam's
/// microphone hears a voice barely above what a headset hears as silence.
const ABOVE_QUIET: f64 = 8.0;
/// Quieter than this, out of 32767, is never a voice: a muted microphone.
const SILENT: f64 = 100.0;
/// Louder than this is a voice however loud the rest is, for one who talks
/// from the first moment to the last.
const LOUD: f64 = 1_000.0;
/// How much of a recording must be voice to count as speech rather than a
/// knock.
const MIN_VOICE: Duration = Duration::from_millis(150);
/// Words to expect, at most: engines cap the list, and a long prompt buys
/// nothing but tokens.
const MAX_TERMS: usize = 50;
/// How long a transcription may take before it is given up on.
const TIMEOUT: Duration = Duration::from_secs(90);

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
    // A slip of Space is not something said, and a muted microphone or a
    // knock isn't either; a made-up word or two would come back for any of
    // them.
    if spoken(&pcm) < MIN_SPOKEN {
        return Ok(String::new());
    }
    if voiced(&pcm) < MIN_VOICE {
        bail!("no voice in that; is the microphone muted or turned down?");
    }
    let key = crate::providers::first(crate::providers::Kind::OpenRouter)
        .map(|p| p.key)
        .filter(|k| !k.is_empty())
        .with_context(|| {
            format!(
                "speech to text is OpenRouter's: add an openrouter provider to {}",
                crate::providers::path().display()
            )
        })?;
    let model = crate::providers::settings()
        .voice_model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let mut body = json!({
        "model": model,
        // Lower is steadier, and there is nothing to be creative about here.
        "temperature": 0,
        "input_audio": {
            "data": base64::engine::general_purpose::STANDARD.encode(wav(&pcm)),
            "format": "wav",
        },
    });
    if let Some(language) = language() {
        body["language"] = json!(language);
    }
    if let Some(terms) = terms() {
        body["provider"] = json!({"options": terms});
    }
    // Word by word, so a word that doesn't hold up can be dropped; some
    // engines say no to it, and are asked plainly instead.
    body["response_format"] = json!("verbose_json");
    body["timestamp_granularities"] = json!(["segment", "word"]);
    let value = ask(&body, &key).await?;
    if value.get("error").is_some() {
        body["response_format"] = Value::Null;
        body["timestamp_granularities"] = Value::Null;
        let value = ask(&body, &key).await?;
        if value.get("error").is_some() {
            let msg = value["error"]["message"].as_str().unwrap_or("no message");
            bail!("OpenRouter: {msg}");
        }
        return Ok(said_text(value["text"].as_str().unwrap_or_default()));
    }
    Ok(said_text(&heard(&value)))
}

/// One transcription request; a refusal comes back in the reply, not as a
/// failed request, since only reading it says whether the model wants the
/// next one asked more plainly.
async fn ask(body: &Value, key: &str) -> Result<Value> {
    let response = client()
        .post(API)
        .bearer_auth(key)
        .header("X-Title", "usecode")
        .json(body)
        .timeout(TIMEOUT)
        .send()
        .await
        .context("calling OpenRouter")?;
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .context("reading OpenRouter's reply")?;
    if !status.is_success() && value.get("error").is_none() {
        bail!("OpenRouter: {status}");
    }
    Ok(value)
}

/// The words that hold up, or the plain text when word by word was refused.
fn heard(value: &Value) -> String {
    let words: Vec<&str> = value["words"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|w| solid(w))
        .filter_map(|w| w["word"].as_str().map(str::trim))
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return value["text"].as_str().unwrap_or_default().to_string();
    }
    words.join(" ")
}

/// Whether a transcribed word is one that was really said: engines score
/// each word from 0 to 1 when they can, and invent a short one now and then.
fn solid(word: &Value) -> bool {
    if word["confidence"].as_f64().is_some_and(|c| c < 0.35) {
        return false;
    }
    match word["word"].as_str() {
        Some(text) => word["start"]
            .as_f64()
            .zip(word["end"].as_f64())
            .map_or_else(
                || !text.trim().is_empty(),
                |(start, end)| {
                    // A "word" the engine grants a few milliseconds to, with no
                    // vowel in it, is noise read as a letter.
                    !(end - start < 0.06 && !text.chars().any(|c| "aeiouyAEIOUY".contains(c)))
                },
            ),
        None => false,
    }
}

/// What was said, or nothing when only punctuation came back.
fn said_text(text: &str) -> String {
    let said = text.trim();
    if said.chars().any(char::is_alphanumeric) {
        said.to_string()
    } else {
        String::new()
    }
}

/// How long the recording is.
fn spoken(pcm: &[u8]) -> Duration {
    Duration::from_secs_f64(pcm.len() as f64 / 2.0 / RATE as f64)
}

/// How much of the recording is a voice: the frames well above its own
/// quiet, which is the quietest tenth of it.
fn voiced(pcm: &[u8]) -> Duration {
    let samples: Vec<f64> = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&s| f64::from(i16::from_le_bytes(s)))
        .collect();
    let mut loudness: Vec<f64> = samples
        .as_chunks::<FRAME>()
        .0
        .iter()
        .map(|frame| (frame.iter().map(|s| s * s).sum::<f64>() / FRAME as f64).sqrt())
        .collect();
    let voice = loudness.clone();
    loudness.sort_by(f64::total_cmp);
    let quiet = loudness
        .get(loudness.len() / 10)
        .copied()
        .unwrap_or_default();
    let frames = voice
        .iter()
        .filter(|&&l| l >= SILENT && (l >= quiet * ABOVE_QUIET || l >= LOUD))
        .count();
    Duration::from_secs_f64((frames * FRAME) as f64 / RATE as f64)
}

/// The language to tell the engine, which guesses poorly from a couple of
/// seconds of speech: `voice: language:` in `agent.yaml`, else the language
/// the user's locale is in.
fn language() -> Option<String> {
    crate::providers::settings()
        .voice_language
        .clone()
        .or_else(|| {
            ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .find_map(|name| locale_language(env(name).as_deref()))
        })
}

/// The language of a locale: `nl_NL.UTF-8` and `de_DE@euro` are `nl` and
/// `de`, and `C` or `POSIX` says nothing about a language.
fn locale_language(locale: Option<&str>) -> Option<String> {
    let language = locale?.split(['_', '.', '@', ':']).next()?.trim();
    (!language.is_empty() && language != "C" && language != "POSIX").then(|| language.to_string())
}

/// The words to expect, under every engine's own name for them; engines
/// offering none ignore the rest. Up to `MAX_TERMS` of `voice: terms:` in
/// `agent.yaml`, the repositories under `~/src` and the commands on PATH;
/// `terms: false` sends none.
fn terms() -> Option<Value> {
    let given = crate::providers::settings().voice_terms.as_ref()?;
    let mut seen = BTreeSet::new();
    let mut words = Vec::new();
    let mut keep = |word: &str| {
        let word = word.trim();
        // A dotfile or a name that isn't a word anyone says is no help.
        if sayable(word) && seen.insert(word.to_string()) && words.len() < MAX_TERMS {
            words.push(word.to_string());
        }
    };
    for term in given {
        keep(term);
    }
    if let Some(src) = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("src")) {
        for repo in repositories(&src) {
            keep(&repo);
        }
    }
    for dir in search_dirs() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for name in entries
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
            {
                keep(&name);
            }
        }
    }
    (!words.is_empty()).then(|| {
        json!({
            "azure": {"phraseList": {"phrases": words}},
            "assemblyai": {"keyterms_prompt": words},
            "deepgram": {"keyterm": words},
            // OpenAI's and Groq's own name for it, where it is a sentence.
            "openai": {"prompt": words.join(", ")},
            "groq": {"prompt": words.join(", ")},
            "mistral": {"prompt": words.join(", ")},
        })
    })
}

/// Whether a name is one to expect in speech: a word of letters, digits and
/// the marks of a command or a file name, not a dotfile or a stray symbol.
fn sayable(word: &str) -> bool {
    word.len() > 1
        && word.starts_with(char::is_alphabetic)
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
}

/// The names of the git repositories under `prefix`, at most a few
/// directories deep, so a checkout's own vendored ones aren't walked.
fn repositories(prefix: &std::path::Path) -> Vec<String> {
    let mut repos = Vec::new();
    let mut dirs = vec![(prefix.to_path_buf(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        if dir.join(".git").exists() {
            if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
                repos.push(name.to_string());
            }
            continue;
        }
        if depth >= 3 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        dirs.extend(
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| (e.path(), depth + 1)),
        );
    }
    repos
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
    fn drops_words_that_do_not_hold_up() {
        // A word the engine is unsure of, and a 30 ms "h" that is noise.
        assert!(!solid(&json!({"word": "h", "start": 1.0, "end": 1.03})));
        assert!(!solid(
            &json!({"word": "kubectl", "start": 1.0, "end": 1.4, "confidence": 0.1})
        ));
        assert!(solid(
            &json!({"word": "kubectl", "start": 1.0, "end": 1.4, "confidence": 0.9})
        ));
        // Without a confidence to go by, only a word too short to be one goes.
        assert!(solid(&json!({"word": "a", "start": 1.0, "end": 1.1})));
    }

    #[test]
    fn keeps_the_words_that_hold_up() {
        let reply = json!({
            "words": [
                {"word": "Run", "start": 0.28, "end": 0.42},
                {"word": "kubectl", "start": 0.52, "end": 1.48, "confidence": 0.9},
                {"word": "h", "start": 1.5, "end": 1.52},
                {"word": "get", "start": 1.64, "end": 1.9, "confidence": 0.95},
            ],
            "text": "Run kubectl h get",
        });
        assert_eq!(heard(&reply), "Run kubectl get");
        // Words are said one after another, with the spaces between them.
        let one = json!({"words": [{"word": " She", "start": 0.12, "end": 0.34}]});
        assert_eq!(heard(&one), "She");
        // Nothing of it holds up, and the plain text is what is left.
        let none = json!({"words": [{"word": "h", "start": 0, "end": 0.02}], "text": "Hm."});
        assert_eq!(heard(&none), "Hm.");
    }

    #[test]
    fn only_punctuation_is_hearing_nothing() {
        assert_eq!(said_text("  git status \n"), "git status");
        assert_eq!(said_text(" . "), "");
        assert_eq!(said_text(""), "");
    }

    #[test]
    fn a_voice_is_told_from_the_quiet_around_it() {
        let enough = |pcm: &[u8]| voiced(pcm) >= MIN_VOICE;
        // A muted microphone, and a quiet room on a webcam's.
        assert!(!enough(&pcm_of(16_000, 0)));
        assert!(!enough(&room(16_000)));
        // A quiet voice on that microphone, softer than a headset's silence.
        let mut said = room(8_000);
        said.extend(pcm_of(8_000, 400));
        assert!(enough(&said));
        // Talking all along, with no quiet to go by.
        assert!(enough(&pcm_of(16_000, 4_000)));
        // A knock, and a cough at the start of an otherwise quiet second.
        let mut knocked = room(16_000);
        knocked.extend(pcm_of(30, 30_000));
        assert!(!enough(&knocked));
        let mut cough = pcm_of(800, 30_000);
        cough.extend(room(16_000));
        assert!(!enough(&cough));
    }

    #[test]
    fn a_slip_of_space_is_not_something_said() {
        assert!(spoken(&pcm_of(1_600, 4_000)) < MIN_SPOKEN);
        assert!(spoken(&pcm_of(16_000, 4_000)) >= MIN_SPOKEN);
    }

    #[test]
    fn the_language_comes_from_the_locale_when_it_is_not_given() {
        assert_eq!(locale_language(Some("nl_NL.UTF-8")), Some("nl".into()));
        assert_eq!(locale_language(Some("de_DE.utf8@euro")), Some("de".into()));
        assert_eq!(locale_language(Some("en_US:en")), Some("en".into()));
        assert_eq!(locale_language(Some("C.UTF-8")), None);
        assert_eq!(locale_language(Some("POSIX")), None);
        assert_eq!(locale_language(None), None);
    }

    #[test]
    fn only_words_are_expected_in_speech() {
        assert!(sayable("kubectl"));
        assert!(sayable("Cargo.toml"));
        assert!(sayable("llama.cpp"));
        assert!(sayable("addr2line"));
        assert!(!sayable("x"));
        assert!(!sayable(".bashrc"));
        assert!(!sayable("["));
        assert!(!sayable("2to3"));
        assert!(!sayable("-version"));
    }

    /// 16-bit PCM of `samples` samples, all at `amplitude`.
    fn pcm_of(samples: usize, amplitude: i16) -> Vec<u8> {
        (0..samples).flat_map(|_| amplitude.to_le_bytes()).collect()
    }

    /// A quiet room as a webcam's microphone hears it: 30 to 190 out of
    /// 32767, back and forth.
    fn room(samples: usize) -> Vec<u8> {
        (0..samples)
            .flat_map(|i| [30i16, -80, 150, -190][i / FRAME % 4].to_le_bytes())
            .collect()
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Transcribes a real WAV against OpenRouter, when given one and a key:
    /// `UC_VOICE_TEST_WAV=rec.wav cargo test -p uc live_voice -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs an OpenRouter key in agent.yaml and a WAV recording"]
    async fn live_voice() {
        let wav = std::env::var("UC_VOICE_TEST_WAV").unwrap();
        let bytes = std::fs::read(wav).unwrap();
        println!("{}", transcribe(bytes[44..].to_vec()).await.unwrap());
    }
}
