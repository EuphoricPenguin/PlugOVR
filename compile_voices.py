#!/usr/bin/env python3
"""Compile OddVoices voice data into .voice files for the Rust PlugOVR synthesizer.

This script reads audio recordings from voice directories (e.g., voices/air/,
voices/cicada/) and compiles them into binary .voice files that can be loaded
by the Rust PlugOVR library.

Usage:
    python compile_voices.py                  # Compile all voices
    python compile_voices.py air              # Compile just the 'air' voice
    python compile_voices.py air cicada       # Compile specific voices

The compiled .voice files are written to PlugOVR/bin/compiled_voices/.
"""

import logging
import pathlib
import sys
import os

# Add the python/ directory to sys.path so we can import oddvoices
SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent  # The oddvoices root directory
PYTHON_DIR = PROJECT_ROOT / "python"

# Insert at the front so it takes precedence
sys.path.insert(0, str(PYTHON_DIR))

import oddvoices.corpus

# ── Paths ──────────────────────────────────────────────────────────────
# Voice source directories (relative to PlugOVR, not project root)
# Each voice is a subdirectory here, e.g.:
#   PlugOVR/voices/air/audio.wav, PlugOVR/voices/air/labels.txt, PlugOVR/voices/air/database.json
VOICES_DIR = SCRIPT_DIR / "voices"

# Output directory for compiled .voice files (inside PlugOVR for Rust to find)
OUTPUT_DIR = SCRIPT_DIR / "bin" / "compiled_voices"


def compile_voice(voice_name: str) -> None:
    """Compile a single voice directory into a .voice file."""
    voice_path = VOICES_DIR / voice_name

    if not voice_path.is_dir():
        print(f"Error: Voice directory not found: {voice_path}")
        return

    print(f"Compiling voice: {voice_name} ({voice_path})")

    try:
        # Analyze the voice audio and extract segments
        segment_database = oddvoices.corpus.CorpusAnalyzer(voice_path).render_database()

        # Write the compiled voice file
        out_file = OUTPUT_DIR / (voice_name + ".voice")
        with open(out_file, "wb") as f:
            oddvoices.corpus.write_voice_file(f, segment_database)

        print(f"  -> {out_file}")
        print(f"  rate={segment_database['rate']}, grain_length={segment_database['grain_length']}")
        print(f"  phonemes: {len(segment_database['phonemes'])}")
        print(f"  segments: {len(segment_database['segments_list'])}")

    except Exception as e:
        print(f"  ERROR: {e}")
        raise


def main():
    logging.basicConfig(level=logging.WARNING)

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    # Determine which voices to compile
    if len(sys.argv) > 1:
        voice_names = sys.argv[1:]
    else:
        # Compile all voice directories
        voice_names = [d.name for d in VOICES_DIR.iterdir() if d.is_dir()]

    for voice_name in voice_names:
        compile_voice(voice_name)

    print(f"\nDone! Compiled {len(voice_names)} voice(s) to {OUTPUT_DIR}")


if __name__ == "__main__":
    main()