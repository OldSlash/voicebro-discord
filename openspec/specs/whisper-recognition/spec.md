# whisper-recognition Specification

## Purpose
Turns what people in the voice channel say into text on this PC with whisper.cpp, on the
CPU or an NVIDIA GPU, from a pinned runtime and a catalog of models downloaded only after
the user agrees, and cuts each speaker's audio into utterances for it.

## Requirements

### Requirement: Whisper runs in the app's process from a pinned build
The app SHALL run whisper.cpp v1.9.5 inside its own process, loading the whisper library and its ggml libraries at run time from the assets folder: `whisper.dll` and the ggml DLLs on Windows, `libwhisper.so` and the ggml shared libraries on Linux (not tested). It SHALL never ship them. Before the first model it SHALL load the backends from the runtime folder (`ggml_backend_load_all_from_path`). It SHALL refuse a whisper library whose `whisper_version()` is not `1.9.5`.

#### Scenario: App starts without Whisper
- **WHEN** the app starts on a PC where nothing of Whisper was ever downloaded
- **THEN** it starts normally, and no Whisper library is loaded until a join in Whisper mode needs one

#### Scenario: A different build in the folder
- **WHEN** the runtime folder holds a whisper library that reports another version
- **THEN** loading fails with a message naming both versions, and nothing calls into that library

### Requirement: Two pinned Whisper runtimes
On Windows two Whisper runtimes SHALL be known, each pinned by SHA-256:

- the CPU build `whisper-bin-x64.zip` (8 928 640 bytes);
- the CUDA 12.4 build `whisper-bin-win-cuda-12.4.0-x64.zip` (684 913 404 bytes).

On Linux x86-64 (not tested) one runtime SHALL be known, pinned by SHA-256: the CPU build `whisper-bin-ubuntu-x64.tar.gz` (10 364 195 bytes). The release has no Linux CUDA build, so Linux has no GPU runtime. Every runtime comes from the whisper.cpp release tag `b5454`. After unpacking, the app SHALL keep only the libraries it loads and delete the rest.

#### Scenario: Unpacked runtime
- **WHEN** the Windows CPU runtime has been downloaded and unpacked
- **THEN** its folder holds `whisper.dll`, `ggml.dll`, `ggml-base.dll` and the `ggml-cpu-*.dll` variants, and no `.exe`

#### Scenario: Unpacked Linux runtime (not tested)
- **WHEN** the Linux (not tested) runtime has been downloaded and unpacked
- **THEN** its folder holds only the shared libraries the app loads, as the probe of the archive found them, and no program

### Requirement: Whisper on the GPU or the CPU
`whisper.device` SHALL choose between the CPU and an NVIDIA GPU. With `gpu`, the app SHALL
use the CUDA runtime and load the model with the GPU enabled. With `cpu`, it SHALL keep the
model and the computation on the CPU, and SHALL create no CUDA device, even when the CUDA
runtime is on disk.

#### Scenario: CPU chosen with the GPU runtime present
- **WHEN** both runtimes are on disk and `whisper.device` is `cpu`
- **THEN** the model loads on the CPU and the process uses no GPU memory for Whisper

#### Scenario: Thread count
- **WHEN** `whisper.threads` is 6 and the device is the CPU
- **THEN** transcription runs with 6 threads, and 0 uses the automatic value the design names

### Requirement: Whisper falls back to the CPU without a CUDA device
When the GPU is requested and no CUDA device is available, the app SHALL load Whisper on the CPU and SHALL say so in the log, the status and, on the desktop app, the Settings tab's Whisper card, naming the requested device, the device used and the reason. On Linux (not tested), where there is no CUDA runtime, a GPU request SHALL always take this fallback.

#### Scenario: GPU requested on a PC without an NVIDIA GPU
- **WHEN** `whisper.device` is `gpu` and the PC has no NVIDIA driver
- **THEN** Whisper runs on the CPU, the log reads "Whisper device: gpu requested, cpu used: no CUDA device", and the Settings tab's Whisper card shows the same

#### Scenario: GPU requested on Linux (not tested)
- **WHEN** `whisper.device` is `gpu` on Linux (not tested)
- **THEN** Whisper runs on the CPU and the log names the requested device, the CPU used and that Linux has no CUDA runtime

### Requirement: Whisper model catalog
The app SHALL offer the catalog of the design (D9):

- tiny, base and small, multilingual and, for base and small, English-only;
- medium;
- large-v3-turbo in three sizes;
- large-v3;
- two Russian-tuned models;
- one English distilled model.

Each model SHALL be a single file pinned by SHA-256, with its size, its license and a
one-line note for the window.

#### Scenario: Model list
- **WHEN** the window asks for the Whisper models
- **THEN** each catalog entry comes with its group (multilingual, English only, Russian-tuned), size, note, license and whether it is downloaded

#### Scenario: Wrong file
- **WHEN** a downloaded model file does not match its pinned SHA-256
- **THEN** the file is deleted, nothing is loaded, and the error names the file and says it was deleted

### Requirement: The VAD model comes with every Whisper setup
The Silero VAD model `ggml-silero-v6.2.0.bin` SHALL be downloaded with every Whisper setup.

#### Scenario: First Whisper model
- **WHEN** the user downloads a Whisper model on a PC without the VAD model
- **THEN** `ggml-silero-v6.2.0.bin` is downloaded with it

### Requirement: The default Whisper model
The default Whisper model SHALL be `whisper-base-q8_0` on the CPU, until the measurement of
the design changes it.

#### Scenario: No model chosen yet
- **WHEN** the app starts with no settings file
- **THEN** the Whisper model is `whisper-base-q8_0` and the device is the CPU

### Requirement: The user's own Whisper models
The app SHALL list every `.bin` file in the `whisper-models\` folder next to the app as a
model with the id `local:<file name>`, once at start. It SHALL never download, change or
delete anything in that folder. A model from the folder that fails to load SHALL be an
error state with the file named, and its selection SHALL be kept.

#### Scenario: A fine-tune added by the user
- **WHEN** the user puts `my-model.bin` into `whisper-models\` and restarts the app
- **THEN** the model list has "my-model.bin" under Your models, and choosing it downloads nothing but the runtime and the VAD model when they are missing

### Requirement: Speech is cut into utterances
In Whisper mode the app SHALL collect each speaker's audio at 16 kHz mono and end an
utterance when that speaker has sent nothing for 35 receive ticks (700 ms). It SHALL cut an
utterance at 28 s, and the rest SHALL start a new one. An utterance with fewer than 12
frames whose peak is 700 or more SHALL be dropped without transcription. Silence SHALL be
counted in receive ticks, never on a clock, so a busy transcriber does not end an utterance
early.

#### Scenario: A pause inside a sentence
- **WHEN** a speaker pauses for 300 ms in the middle of a sentence and goes on
- **THEN** the sentence is one utterance

#### Scenario: End of a question
- **WHEN** a speaker stops talking
- **THEN** the utterance is handed to the transcriber 700 ms after the last frame

#### Scenario: A click
- **WHEN** a speaker's microphone sends 100 ms of a click and nothing else
- **THEN** nothing is transcribed and the drop is counted as noise

### Requirement: Transcription queue
Utterances SHALL be transcribed one at a time on a thread of their own, separate from the
thread that cuts them. At most 8 utterances SHALL wait. When a ninth arrives, the oldest
waiting one SHALL be dropped, counted and logged with its length. Receiving audio SHALL
never wait for a transcription.

#### Scenario: A slow model under load
- **WHEN** four people talk at once on a CPU model that needs 2 s per utterance
- **THEN** no Discord audio is delayed, the queue never holds more than 8 utterances, and each drop appears in the log and the status

### Requirement: Noise does not become text
Each utterance SHALL pass Whisper's built-in voice activity detection, with the Silero model
of the catalog. An utterance without speech segments SHALL give no text. A segment whose
no-speech probability is above 0.6 SHALL be dropped. A text that repeats one word four or
more times in a row SHALL be dropped as a hallucination only when that run has more words
than five per second of the utterance's speech. Each kind of drop SHALL be counted in the
status, and a dropped hallucination SHALL be logged with the number of repeats and the
utterance's length.

#### Scenario: Game sound on an open microphone
- **WHEN** a speaker's microphone sends 3 s of game sound with no speech
- **THEN** no text comes out, no phrase counts, and the noise drop counter rises

#### Scenario: Silence with phrases in the prompt
- **WHEN** the noise-only clips of the corpus are transcribed with the activation phrases in the prompt
- **THEN** none of them produces an activation phrase

#### Scenario: A word said several times
- **WHEN** a speaker says "bro, bro, bro, bro" in 1.5 s and Whisper writes it so
- **THEN** the text is kept, and the activation phrase "bro" in it counts

#### Scenario: A decoding loop
- **WHEN** Whisper writes one word twenty times in a row for an utterance of 2 s
- **THEN** the text is dropped as a hallucination, the hallucination counter rises, and the log line names 20 repeats in 2.0 s

### Requirement: Whisper loads at a join and unloads after the last leave
In Whisper mode, the model, the VAD model and the runtime SHALL load while the bot joins
a voice channel. They SHALL unload 0.5 s after the bot has left its last voice channel,
and when the assistant mode goes back to realtime; the runtime's libraries stay loaded for
the process. A leave SHALL stop the transcription in progress of that guild, and an unload
any transcription in progress, at the next point whisper.cpp checks: the end of the
encoder pass or decoder step running. The text of a stopped transcription SHALL be
dropped. After a model on the GPU is freed, the app SHALL reset the process's CUDA context,
so the GPU memory it held goes back to the system. A change of model, device, language or
thread count while loaded SHALL load the new model in the background and SHALL keep the old
one in use until the new one is ready. The load time and the device SHALL be logged.

#### Scenario: Idle between joins
- **WHEN** the app runs in Whisper mode and the bot is in no voice channel
- **THEN** no Whisper model is in memory, and no CUDA context holds GPU memory

#### Scenario: Model changed during a session
- **WHEN** the user picks another model while the bot is in a channel
- **THEN** questions keep being transcribed with the old model until the new one has loaded, and the log names the switch

#### Scenario: Leave during a transcription on the CPU
- **WHEN** the bot leaves while Whisper on the CPU transcribes an utterance of that guild
- **THEN** the transcription stops at the end of the encoder pass or decoder step running, no text of it comes out, the log says it was stopped, and the model unloads after that

#### Scenario: GPU memory after the leave
- **WHEN** the bot leaves its last voice channel while a model is loaded on the GPU
- **THEN** after the unload the app holds no GPU memory, and the log says the CUDA context was reset

### Requirement: Whisper diagnostics
The status SHALL show Whisper's state, the model, the device requested and the device in
use with the reason for any fallback, the backend, the load time, the utterances queued,
transcribed and dropped by kind, and the last transcription's time and audio length. The
log SHALL keep ggml's buffer-size lines from the model load at info level. What Whisper
heard SHALL be a detail line, written to the file only with the verbose log.

#### Scenario: Reading the GPU memory a model holds
- **WHEN** a model is loaded on the GPU
- **THEN** the log has ggml's lines with the model's, the KV cache's and the compute buffers' sizes on the device
