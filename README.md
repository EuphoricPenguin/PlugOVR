## PlugOVR

**A pure Rust port of OddVoices, a singing synthesizer for General American English, based on diphone concatenation with PSOLA (Pitch Synchronous Overlap Add).**

For now, this is a port of the synth core and CLI until some minor bugs can be corrected. By and large, though, this is a working port of [OddVoices](https://gitlab.com/oddvoices/oddvoices/) to Rust.

To get it working, copy the [voices](https://gitlab.com/oddvoices/oddvoices/-/tree/develop/voices?ref_type=heads) folder to the main directory after cloning this repo. Install Python and Rustup. Run `pip -r requirements.txt` and  `python compile_voices.py` to build the voice binaries. After that, run `cargo build` to build the project. If that succeeded, run `[PlugOVR binary] sing bin\compiled_voices\cicada.voice bin\cmudict-0.7b output\test.wav -m testfiles\test.mid -l testfiles\lyrics.txt` to test the output. `[PlugOVR binary] --help` has more details on the CLI usage.

<sub>PlugOVR was created using a fully-local LLM toolchain consisting of Cline, Qwen-3.6-35B-A3B, and Grounded Docs MCP Server with Granite-Embedding-278m-multilingual.</sub>