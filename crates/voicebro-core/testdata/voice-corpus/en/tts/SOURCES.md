# Where the voices of this folder come from

Every WAV here is machine generated speech, made by `probe voice-corpus-make` (change
`voice-corpus-tests`); none is a recording of a person. The voices were chosen because
their licenses allow publishing what they say. Each license below was read from the
voice's model card and, for the datasets, from their own pages on 2026-11-23.

| Files | Voice | License | Model card |
|---|---|---|---|
| `ljspeech-*.wav` | Piper en_US-ljspeech-high (American, female) | public domain: trained from scratch on LJ Speech, which has no restrictions on its use | https://huggingface.co/rhasspy/piper-voices/blob/main/en/en_US/ljspeech/high/MODEL_CARD |
| `cori-*.wav` | Piper en_GB-cori-high (British, female) | public domain: trained from scratch on LibriVox recordings | https://huggingface.co/rhasspy/piper-voices/blob/main/en/en_GB/cori/high/MODEL_CARD |
| `bella-*.wav` | Kokoro v0.19 af_bella (American, female) | Apache-2.0 (Kokoro-82M v0.19; trained on permissive or non-copyrighted audio, its card says) | https://huggingface.co/hexgrad/Kokoro-82M |
| `adam-*.wav` | Kokoro v0.19 am_adam (American, male) | Apache-2.0 (Kokoro-82M v0.19; trained on permissive or non-copyrighted audio, its card says) | https://huggingface.co/hexgrad/Kokoro-82M |
| `emma-*.wav` | Kokoro v0.19 bf_emma (British, female) | Apache-2.0 (Kokoro-82M v0.19; trained on permissive or non-copyrighted audio, its card says) | https://huggingface.co/hexgrad/Kokoro-82M |
| `george-*.wav` | Kokoro v0.19 bm_george (British, male) | Apache-2.0 (Kokoro-82M v0.19; trained on permissive or non-copyrighted audio, its card says) | https://huggingface.co/hexgrad/Kokoro-82M |

The voices run on sherpa-onnx; Piper's and Kokoro's text goes through piper-phonemize
(espeak-ng, GPL-3.0), whose license does not cover the audio a separate model makes.
