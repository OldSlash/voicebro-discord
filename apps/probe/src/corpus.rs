//! `probe voice-corpus-make` (change `voice-corpus-tests`, design D2): records the English
//! set of the voice corpus into `crates/voicebro-core/testdata/voice-corpus/en/tts/` with
//! voices whose output may be published: Piper ljspeech (LJ Speech) and Piper cori (LibriVox
//! recordings), both public domain and trained from scratch, and four Kokoro v0.19 voices
//! (Apache-2.0). Each file is 16 kHz mono 16-bit; `expect.txt` and `SOURCES.md` name the
//! voices and their licenses. It replaces that folder whole and touches nothing else.

use crate::whisper::{assets_dir, ensure};
use crate::Outcome;
use anyhow::{anyhow, Context};
use std::path::{Path, PathBuf};
use voicebro_core::assets::Asset;
use voicebro_core::audio::dsp::Resampler;
use voicebro_core::tts::catalog;
use voicebro_core::tts::manager::{Loader, RealLoader};
use voicebro_core::tts::sherpa::{Model, SherpaVoice};
use voicebro_core::tts::Synth;

/// One case: its file name part, the sentences (a 1.5 s pause between two), the expectation.
struct Case {
    name: &'static str,
    sentences: &'static [&'static str],
    expect: &'static str,
}

/// The cases of design D2; the first nine sentences are the old English set's.
const CASES: &[Case] = &[
    Case {
        name: "bro-alone",
        sentences: &["Bro."],
        expect: "activate",
    },
    Case {
        name: "bro-start",
        sentences: &["Bro, what time is it in London?"],
        expect: "activate",
    },
    Case {
        name: "hey-bro",
        sentences: &["Hey bro, what is the weather in Bangkok?"],
        expect: "activate",
    },
    Case {
        name: "bro-end",
        sentences: &["That was a good game, bro."],
        expect: "activate",
    },
    Case {
        name: "plain",
        sentences: &["Let's go to the castle and wait for the others."],
        expect: "none",
    },
    Case {
        name: "brother",
        sentences: &["Brother, tell me a joke."],
        expect: "none",
    },
    Case {
        name: "lookalike",
        sentences: &["I broke my bow, he brought the brown boat, nice bros."],
        expect: "none",
    },
    Case {
        name: "peace-alone",
        sentences: &["Peace out."],
        expect: "deactivate",
    },
    Case {
        name: "goodbye",
        sentences: &["Thanks for the help. Peace out."],
        expect: "deactivate",
    },
    Case {
        name: "bro-then-peace",
        sentences: &["Bro, what time is it in London?", "Thanks, peace out."],
        expect: "activate, deactivate",
    },
];

const PAUSE_SECONDS: f32 = 1.5;
/// The rate of every file of the set.
const RATE: u32 = 16_000;

/// Piper's en_US-ljspeech-high as sherpa-onnx packs it. Not in the app's voice list, so the
/// probe pins it itself (hash and size of the archive read on 2026-11-23).
const LJSPEECH: Asset = Asset {
    label: "Piper voice en_US-ljspeech-high",
    page: "https://github.com/k2-fsa/sherpa-onnx/releases/tag/tts-models",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-ljspeech-high.tar.bz2",
    sha256: "00c6408d2409050312193b0d40ae07fde28af7d3d45a56efcc55440db516b935",
    directory: "vits-piper-en_US-ljspeech-high",
    marker: "en_US-ljspeech-high.onnx",
    size: 115_817_679,
    ..Asset::ARCHIVE
};

/// Where a voice comes from: a voice of the app's list, or the probe's ljspeech.
enum Source {
    Catalog(&'static str),
    Ljspeech,
}

/// One voice of the set: the file name part, what it is, and why its output may be
/// published (read from the model cards and dataset pages on 2026-11-23).
struct CorpusVoice {
    name: &'static str,
    source: Source,
    voice: &'static str,
    license: &'static str,
    card: &'static str,
}

const KOKORO_LICENSE: &str =
    "Apache-2.0 (Kokoro-82M v0.19; trained on permissive or non-copyrighted audio, its card says)";
const KOKORO_CARD: &str = "https://huggingface.co/hexgrad/Kokoro-82M";

const VOICES: &[CorpusVoice] = &[
    CorpusVoice {
        name: "ljspeech",
        source: Source::Ljspeech,
        voice: "Piper en_US-ljspeech-high (American, female)",
        license: "public domain: trained from scratch on LJ Speech, which has no restrictions on its use",
        card: "https://huggingface.co/rhasspy/piper-voices/blob/main/en/en_US/ljspeech/high/MODEL_CARD",
    },
    CorpusVoice {
        name: "cori",
        source: Source::Catalog("piper:en_GB-cori-high"),
        voice: "Piper en_GB-cori-high (British, female)",
        license: "public domain: trained from scratch on LibriVox recordings",
        card: "https://huggingface.co/rhasspy/piper-voices/blob/main/en/en_GB/cori/high/MODEL_CARD",
    },
    CorpusVoice {
        name: "bella",
        source: Source::Catalog("kokoro-en:af_bella"),
        voice: "Kokoro v0.19 af_bella (American, female)",
        license: KOKORO_LICENSE,
        card: KOKORO_CARD,
    },
    CorpusVoice {
        name: "adam",
        source: Source::Catalog("kokoro-en:am_adam"),
        voice: "Kokoro v0.19 am_adam (American, male)",
        license: KOKORO_LICENSE,
        card: KOKORO_CARD,
    },
    CorpusVoice {
        name: "emma",
        source: Source::Catalog("kokoro-en:bf_emma"),
        voice: "Kokoro v0.19 bf_emma (British, female)",
        license: KOKORO_LICENSE,
        card: KOKORO_CARD,
    },
    CorpusVoice {
        name: "george",
        source: Source::Catalog("kokoro-en:bm_george"),
        voice: "Kokoro v0.19 bm_george (British, male)",
        license: KOKORO_LICENSE,
        card: KOKORO_CARD,
    },
];

const HEADER: &str = "\
# The English set of the voice corpus (change voice-corpus-tests), recorded by
# `probe voice-corpus-make` with the voices SOURCES.md names: machine generated speech whose
# voices allow publishing it. The probe rewrites this folder whole: put your own files in
# another folder, such as en/mine/.
#
# The format, for this folder and your own:
#   phrases = <activation, ...> | <deactivation, ...>
#     optional; applies to this folder and the folders below it that set none
#     (without it: bro | peace out)
#   <file>.wav = activate | deactivate | activate, deactivate | none [; the words spoken]
#     the state changes the file must make, in order; a list that begins with deactivate
#     starts with a session active
# A WAV without a line is run and reported, not judged. Files under a folder named local
# are never committed: the repository is public.
";

fn corpus_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("voicebro-core")
        .join("testdata")
        .join("voice-corpus")
        .join("en")
        .join("tts")
}

/// 16-bit mono PCM.
fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|&value| ((value * 32_767.0).clamp(-32_768.0, 32_767.0) as i16).to_le_bytes())
        .collect();
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + data.len() as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * 2).to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((data.len() as u32).to_le_bytes());
    bytes.extend(data);
    bytes
}

/// `audio` at `from` Hz as 16 kHz.
fn to_rate(audio: &[f32], from: u32) -> Vec<f32> {
    if from == RATE {
        return audio.to_vec();
    }
    let mut resampler = Resampler::new(from, RATE);
    let mut out = resampler.process(audio);
    out.extend(resampler.finish());
    out
}

/// Installs the voice's files into `dir` (a missing one is downloaded with its pinned hash)
/// and loads it.
fn load(
    voice: &CorpusVoice,
    dir: &Path,
    details: &mut Vec<String>,
) -> anyhow::Result<Box<dyn Synth>> {
    match voice.source {
        Source::Catalog(id) => {
            let entry = catalog::find(id).ok_or_else(|| anyhow!("{id} is not a voice"))?;
            for asset in catalog::assets_for(entry) {
                ensure(&asset, dir, details)?;
            }
            RealLoader::default()
                .load(id, dir)
                .map_err(|error| anyhow!("{}: {error}", voice.voice))
        }
        Source::Ljspeech => {
            for asset in [catalog::ENGINE, catalog::PHONEMIZER, LJSPEECH] {
                ensure(&asset, dir, details)?;
            }
            let model = Model::Piper {
                folder: dir.join(LJSPEECH.directory),
                model: LJSPEECH.marker.to_owned(),
            };
            let synth = SherpaVoice::load(
                &dir.join(catalog::ENGINE.directory),
                Some(&dir.join(catalog::PHONEMIZER.directory)),
                model,
                0,
                voicebro_core::tts::manager::threads_for(catalog::VoiceEngine::Piper),
            )
            .map_err(|error| anyhow!("{}: {error}", voice.voice))?;
            Ok(Box::new(synth))
        }
    }
}

/// The folder's `SOURCES.md`: every file is machine generated, and which voice made it.
fn sources() -> String {
    let mut text = String::from(
        "# Where the voices of this folder come from\n\n\
         Every WAV here is machine generated speech, made by `probe voice-corpus-make` (change\n\
         `voice-corpus-tests`); none is a recording of a person. The voices were chosen because\n\
         their licenses allow publishing what they say. Each license below was read from the\n\
         voice's model card and, for the datasets, from their own pages on 2026-11-23.\n\n\
         | Files | Voice | License | Model card |\n|---|---|---|---|\n",
    );
    for voice in VOICES {
        text.push_str(&format!(
            "| `{}-*.wav` | {} | {} | {} |\n",
            voice.name, voice.voice, voice.license, voice.card
        ));
    }
    text.push_str(
        "\nThe voices run on sherpa-onnx; Piper's and Kokoro's text goes through piper-phonemize\n\
         (espeak-ng, GPL-3.0), whose license does not cover the audio a separate model makes.\n",
    );
    text
}

/// `probe voice-corpus-make [--assets <dir>]`: the voices' files come from `--assets`,
/// `VOICEBRO_WHISPER_DIR` or `target\whisper-test\assets`, a missing one downloaded with its
/// pinned hash.
pub fn make(args: &[String]) -> anyhow::Result<Outcome> {
    let dir = assets_dir(args);
    let mut details = Vec::new();
    let mut synths = Vec::new();
    for voice in VOICES {
        synths.push(load(voice, &dir, &mut details)?);
    }
    let folder = corpus_folder();
    if folder.exists() {
        std::fs::remove_dir_all(&folder)
            .with_context(|| format!("removing {}", folder.display()))?;
    }
    std::fs::create_dir_all(&folder).with_context(|| format!("creating {}", folder.display()))?;
    let mut expect = HEADER.to_owned();
    for (voice, synth) in VOICES.iter().zip(synths.iter_mut()) {
        let rate = synth.sample_rate();
        for case in CASES {
            let mut audio = Vec::new();
            for (index, sentence) in case.sentences.iter().enumerate() {
                if index > 0 {
                    audio.extend(std::iter::repeat_n(
                        0.0,
                        (PAUSE_SECONDS * RATE as f32) as usize,
                    ));
                }
                let spoken = synth
                    .synthesize(sentence, Some("en"), 1.0, &|| false)
                    .map_err(|error| anyhow!("{}: {error}", voice.voice))?
                    .ok_or_else(|| anyhow!("{}: the synthesis stopped", voice.voice))?;
                audio.extend(to_rate(&spoken, rate));
            }
            let file = format!("{}-{}.wav", voice.name, case.name);
            std::fs::write(folder.join(&file), wav(&audio, RATE))
                .with_context(|| format!("writing {file}"))?;
            expect.push_str(&format!(
                "{file} = {} ; {}\n",
                case.expect,
                case.sentences.join(" ")
            ));
            details.push(format!(
                "{file}: {:.2} s ({} at {rate} Hz)",
                audio.len() as f64 / f64::from(RATE),
                voice.voice
            ));
        }
    }
    std::fs::write(folder.join("expect.txt"), expect).context("writing expect.txt")?;
    std::fs::write(folder.join("SOURCES.md"), sources()).context("writing SOURCES.md")?;
    details.push(format!(
        "{} files from {} voices in {}",
        VOICES.len() * CASES.len(),
        VOICES.len(),
        folder.display()
    ));
    Ok(Outcome::pass(details))
}
