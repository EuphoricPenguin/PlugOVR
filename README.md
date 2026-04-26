## PlugOVR

**A pure Rust port of OddVoices, a singing synthesizer for General American English, based on diphone concatenation with PSOLA (Pitch Synchronous Overlap Add).**

This is a port of the synth core and CLI of [OddVoices](https://gitlab.com/oddvoices/oddvoices/) to Rust.

To get it working, copy the [voices](https://gitlab.com/oddvoices/oddvoices/-/tree/develop/voices?ref_type=heads) folder to the main directory after cloning this repo. Install Python and Rustup. Run `pip -r requirements.txt` and  `python compile_voices.py` to build the voice binaries. After that, run `cargo build` to build the project. If that succeeded, run `[PlugOVR binary] sing bin\compiled_voices\cicada.voice bin\cmudict-0.7b output\test.wav -m testfiles\test.mid -l testfiles\lyrics.txt` to test the output. `[PlugOVR binary] --help` has more details on the CLI usage.

### Licensing
PlugOVR, a derivative of OddVoices, is licensed under the Apache-2.0 License.
OddVoices is (c) 2021 Nathan Ho and licensed under the Apache-2.0 License. See LICENSE for more information.
The [voice source files](https://gitlab.com/oddvoices/oddvoices/-/tree/develop/voices?ref_type=heads), available in the original repo, are dedicated to the public domain via CC0.
The CMU Pronouncing Dictionary is copyright (c) 1993-2015 Carnegie Mellon University, and licensed under the BSD 2-Clause License.

<sub>The 0.1 version of PlugOVR was created using a fully-local LLM toolchain consisting of Cline, Qwen-3.6-35B-A3B, and Grounded Docs MCP Server with Granite-Embedding-278m-multilingual. 0.2 had several minor issues fixed using DeepSeek V4 Flash.</sub>