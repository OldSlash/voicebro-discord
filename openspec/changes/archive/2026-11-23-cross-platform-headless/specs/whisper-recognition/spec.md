# Spec Delta

## MODIFIED Requirements

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

### Requirement: Whisper falls back to the CPU without a CUDA device
When the GPU is requested and no CUDA device is available, the app SHALL load Whisper on the CPU and SHALL say so in the log, the status and, on the desktop app, the Settings tab's Whisper card, naming the requested device, the device used and the reason. On Linux (not tested), where there is no CUDA runtime, a GPU request SHALL always take this fallback.

#### Scenario: GPU requested on a PC without an NVIDIA GPU
- **WHEN** `whisper.device` is `gpu` and the PC has no NVIDIA driver
- **THEN** Whisper runs on the CPU, the log reads "Whisper device: gpu requested, cpu used: no CUDA device", and the Settings tab's Whisper card shows the same

#### Scenario: GPU requested on Linux (not tested)
- **WHEN** `whisper.device` is `gpu` on Linux (not tested)
- **THEN** Whisper runs on the CPU and the log names the requested device, the CPU used and that Linux has no CUDA runtime
