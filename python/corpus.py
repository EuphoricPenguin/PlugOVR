import json
import logging
import pathlib
import struct

import soundfile
import scipy.optimize
import scipy.signal
import scipy.stats
import numpy as np
import oddvoices.phonology


AUTOCORRELATION_WINDOW_SIZE_NUMBER_OF_PERIODS = 8
RANDOMIZED_PHASE_CUTOFF = 3000.0

SEMITONE = 2 ** (1 / 12)


def db_to_linear(db):
    return 10 ** (db / 20)


def linear_to_db(linear):
    return 20 * np.log10(linear)


def midi_note_to_hertz(midi_note):
    return 440 * 2 ** ((midi_note - 69) / 12)


def seconds_to_timestamp(seconds):
    minutes = int(seconds / 60)
    remaining_seconds = seconds - minutes * 60
    return f"{minutes}:{remaining_seconds:.02f}"


class CorpusAnalyzer:
    def __init__(self, directory):
        root = pathlib.Path(directory)
        sound_file = root / "audio.wav"
        label_file = root / "labels.txt"
        info_file = root / "database.json"

        with open(info_file) as f:
            info = json.load(f)

        self.expected_f0: float = midi_note_to_hertz(info["f0_midi_note"])
        self.audio: np.array
        self.rate: int
        self.audio, self.rate = soundfile.read(sound_file)

        self.period = int(round(self.rate / self.expected_f0))

        self.n_randomized_phases = int(RANDOMIZED_PHASE_CUTOFF / self.expected_f0)
        np.random.seed(0)
        self.randomized_phases = np.exp(
            np.random.random((self.n_randomized_phases,)) * 2 * np.pi * 1j
        )

        self.last_f0 = self.expected_f0

        self.lowpass_filter = scipy.signal.remez(
            numtaps=128,
            bands=[0, 900, 3000, self.rate / 2],
            desired=[1, 0],
            fs=self.rate,
        )

        self.parse_label_file(label_file)

    def parse_label_file(self, label_file):
        self.markers = {}
        with open(label_file) as label_file:
            for line in label_file:
                entries = line.strip().split(maxsplit=2)
                if len(entries) == 3:
                    start, end, text = entries
                    start = float(start)
                    end = float(end)
                    segment_id = tuple(oddvoices.phonology.parse_pronunciation(text))

                    self.markers[segment_id] = {
                        "start": int(float(start) * self.rate),
                        "end": int(float(end) * self.rate),
                    }

    def get_f0(self, offset):
        window_size: int = int(
            self.period * AUTOCORRELATION_WINDOW_SIZE_NUMBER_OF_PERIODS
        )

        start: int = offset - window_size // 2
        end: int = start + window_size

        frame: np.array = self.audio[start:end]
        # Apply lowpass filter.
        frame = scipy.signal.convolve(frame, self.lowpass_filter, mode="same")
        # Apply nonlinear function that squeezes towards 0.
        frame = frame * (np.abs(frame) > np.max(np.abs(frame)) * 0.5)
        frame: np.array = frame * scipy.signal.get_window("hann", window_size)

        autocorrelation: np.array = scipy.signal.correlate(frame, frame)
        autocorrelation = autocorrelation[window_size:]
        ascending_bins = np.where(np.diff(autocorrelation) >= 0)
        if len(ascending_bins[0]) == 0:
            return None
        first_ascending_bin = np.min(ascending_bins)
        measured_period: int = (
            np.argmax(autocorrelation[first_ascending_bin:]) + first_ascending_bin
        )
        measured_f0 = self.rate / measured_period

        if not self.expected_f0 / SEMITONE < measured_f0 < self.expected_f0 * SEMITONE:
            return None

        return measured_f0

    def get_frame_dps_center(self, frame):
        """Get the center of a frame using the differentiated phase spectrum (DPS),
        in samples."""
        n = np.arange(len(frame)) - len(frame) / 2
        omega = 2 * np.pi / (len(frame) / 2)
        return np.angle(np.sum(frame * frame * np.exp(1j * n * omega))) / omega

    def analyze_psola(self, start, end):
        frames = []
        dps_center_values = []
        offset: int = start
        while offset <= end:
            f0 = self.get_f0(offset)
            voiced = f0 is not None
            if not voiced:
                f0 = self.expected_f0
            measured_period = self.rate / f0 if voiced else self.period
            window_size: int = int(measured_period * 2)
            frame_start: int = offset - window_size // 2
            frame_end: int = frame_start + window_size
            frame: np.array = self.audio[frame_start:frame_end]
            frame = frame * scipy.signal.get_window("hann", len(frame))
            frame = scipy.signal.resample(frame, int(self.period * 2))
            frames.append(
                {
                    "frame": frame,
                    "f0": f0,
                    "voiced": voiced,
                    "start": frame_start,
                    "end": frame_end,
                }
            )
            offset += int(round(measured_period))

        return np.array(frames)

    def make_loopable(self, frames):
        crossfade_length = 0.1
        crossfade_length_in_frames = int(round(crossfade_length * self.expected_f0))
        t = np.linspace(0, 1, crossfade_length_in_frames, endpoint=False)
        fade_in = frames[:crossfade_length_in_frames, :] * t[:, np.newaxis]
        fade_out = frames[-crossfade_length_in_frames:, :] * (1 - t[:, np.newaxis])
        fade = fade_in + fade_out
        stable = frames[crossfade_length_in_frames:-crossfade_length_in_frames, :]
        return np.vstack([fade, stable])

    def process_segment(self, segment_id):
        logging.info(f"Processing segment {segment_id}.")
        is_vowel = len(segment_id) == 1 and segment_id[0] in oddvoices.phonology.VOWELS
        markers = self.markers[segment_id]
        frames_info = self.analyze_psola(markers["start"], markers["end"])

        if is_vowel:
            num_unvoiced_frames = 0
            for frame_info in frames_info:
                if not frame_info["voiced"]:
                    num_unvoiced_frames += 1
            if num_unvoiced_frames > 0:
                raise RuntimeError(
                    f"{num_unvoiced_frames}/{len(frames_info)} unvoiced frames found "
                    f"in vowel segment {segment_id}."
                )

        dps_center_values = []
        frames = []
        for frame_info in frames_info:
            frame_start, frame_end = frame_info["start"], frame_info["end"]
            frame = frame_info["frame"]
            if frame_info["voiced"]:
                dps_center = int(round(self.get_frame_dps_center(frame)))
                dps_center_values.append(dps_center)
            frames.append(frame)
        frames = np.array(frames)
        dps_center_values = np.array(dps_center_values)

        smoothed_dps_center_values = dps_center_values
        if len(dps_center_values) > 1:
            # Apply a lowpass filter to the circular signal dps_center_values
            # by converting it into sin/cos, lowpassing each individually, and
            # using arctan2 to find the angle again.
            cos = np.cos(dps_center_values * 2 * np.pi / self.period)
            sin = np.sin(dps_center_values * 2 * np.pi / self.period)
            filter_length = 50
            filter_ = np.ones((filter_length,))
            padded_cos = np.concatenate(
                [
                    np.ones((filter_length,)) * cos[0],
                    cos,
                    np.ones((filter_length,)) * cos[-1],
                ]
            )
            padded_sin = np.concatenate(
                [
                    np.ones((filter_length,)) * sin[0],
                    sin,
                    np.ones((filter_length,)) * sin[-1],
                ]
            )
            padded_cos = scipy.signal.convolve(padded_cos, filter_, mode="same")
            padded_sin = scipy.signal.convolve(padded_sin, filter_, mode="same")
            cos = padded_cos[filter_length:-filter_length]
            sin = padded_sin[filter_length:-filter_length]
            smoothed_dps_center_values = (
                np.arctan2(sin, cos) * self.period / (2 * np.pi)
            )

        i = 0
        frames = []
        for frame_info in frames_info:
            frame_start, frame_end = frame_info["start"], frame_info["end"]
            frame = frame_info["frame"]
            if frame_info["voiced"]:
                dps_center = smoothed_dps_center_values[i]
                dps_center = dps_center % self.period
                if dps_center >= self.period / 2:
                    dps_center -= self.period
                dps_center = int(round(dps_center))
                i += 1
                frame = self.audio[frame_start + dps_center : frame_end + dps_center]
                frame = frame * scipy.signal.get_window("hann", len(frame))
                frame = scipy.signal.resample(frame, int(self.period * 2))
                frames.append(frame)
            frames.append(frame)
        frames = np.array(frames)

        if is_vowel:
            frames = self.make_loopable(frames)

        return {
            "frames": frames,
            "num_frames": len(frames),
            "vowel": is_vowel,
        }

    def normalize_segment_amplitudes(self):
        for phoneme in oddvoices.phonology.ALL_PHONEMES:
            if phoneme == "_":
                continue

            segments_ending_with_phoneme = []
            segments_beginning_with_phoneme = []
            for segment_id in self.markers.keys():
                if len(segment_id) == 2 and segment_id[1] == phoneme:
                    segments_ending_with_phoneme.append(segment_id)
                if len(segment_id) == 2 and segment_id[0] == phoneme:
                    segments_beginning_with_phoneme.append(segment_id)

            edge_size = 10

            squared_rmss = []
            ending_rms_by_segment_id = {}
            beginning_rms_by_segment_id = {}
            vowel_rms = 1

            if phoneme in self.database["segments"]:
                tmp = self.database["segments"][phoneme]["frames"]
                squared_rms = np.mean(tmp * tmp)
                squared_rmss.append(squared_rms)
                vowel_rms = np.sqrt(squared_rms)

            for segment_id in segments_ending_with_phoneme:
                segment = self.database["segments"]["".join(segment_id)]
                tmp = segment["frames"][-edge_size:, :]
                squared_rms = np.mean(tmp * tmp)
                squared_rmss.append(squared_rms)
                ending_rms_by_segment_id[segment_id] = np.sqrt(squared_rms)

            for segment_id in segments_beginning_with_phoneme:
                segment = self.database["segments"]["".join(segment_id)]
                tmp = segment["frames"][:edge_size, :]
                squared_rms = np.mean(tmp * tmp)
                squared_rmss.append(squared_rms)
                beginning_rms_by_segment_id[segment_id] = np.sqrt(squared_rms)

            phoneme_rms = np.sqrt(np.mean(squared_rmss))

            if phoneme in self.database["segments"]:
                segment = self.database["segments"][phoneme]
                gain = phoneme_rms / vowel_rms
                segment["beginning_gain"] = segment["ending_gain"] = gain

            for segment_id in segments_ending_with_phoneme:
                segment = self.database["segments"]["".join(segment_id)]
                rms = ending_rms_by_segment_id[segment_id]
                ending_gain = phoneme_rms / rms
                segment["ending_gain"] = ending_gain
                if segment_id[0] == "_":
                    segment["beginning_gain"] = ending_gain

            for segment_id in segments_beginning_with_phoneme:
                segment = self.database["segments"]["".join(segment_id)]
                rms = beginning_rms_by_segment_id[segment_id]
                beginning_gain = phoneme_rms / rms
                segment["beginning_gain"] = beginning_gain
                if segment_id[-1] == "_":
                    segment["ending"] = beginning_gain

        for segment_id in self.markers.keys():
            segment = self.database["segments"]["".join(segment_id)]
            frames = segment["frames"]
            envelope = np.linspace(
                segment.get("beginning_gain", 1),
                segment.get("ending_gain", 1),
                frames.shape[0],
            )
            segment["frames"] = frames * envelope[:, np.newaxis]

    def normalize_database(self):
        sum_of_squares = 0
        total_frames = 0
        for segment_id in sorted(list(self.markers.keys())):
            name = "".join(segment_id)
            frames = self.database["segments"][name]["frames"]
            sum_of_squares += np.sum(frames * frames)
            total_frames += frames.shape[0]
        rms = np.sqrt(sum_of_squares / total_frames / self.database["grain_length"])
        logging.info(f"Overall RMS: {linear_to_db(rms):.2f} dB")
        target = db_to_linear(-20)
        gain = target / rms
        logging.info(f"Overall gain: {linear_to_db(gain):.2f} dB")

        safety_limit = db_to_linear(-6)

        for segment_id in sorted(list(self.markers.keys())):
            name = "".join(segment_id)
            frames = self.database["segments"][name]["frames"]
            frames *= gain
            peaks = np.amax(np.abs(frames), axis=1)
            limiter_gains = 1 / np.maximum(peaks, 1 / safety_limit)
            frames *= limiter_gains[:, np.newaxis]
            frames *= 32767
            frames = frames.astype(np.int16)
            self.database["segments"][name]["frames"] = frames

    def render_database(self):
        self.database = {
            "rate": self.rate,
            "phonemes": oddvoices.phonology.ALL_PHONEMES,
            "grain_length": 2 * int(self.rate / self.expected_f0),
            "segments_list": ["".join(x) for x in sorted(list(self.markers.keys()))],
            "segments": {},
        }

        for segment_id in sorted(list(self.markers.keys())):
            self.database["segments"]["".join(segment_id)] = self.process_segment(
                segment_id
            )
        self.normalize_segment_amplitudes()
        self.normalize_database()
        return self.database


MAGIC_WORD = b"ODDVOICES\0\0\0"


def write_voice_file_header(f, database):
    f.write(MAGIC_WORD)
    f.write(struct.pack("<l", database["rate"]))
    f.write(struct.pack("<l", database["grain_length"]))

    for phoneme in database["phonemes"]:
        f.write(phoneme.encode("ascii") + b"\0")
    f.write(b"\0")

    for segment_name in database["segments_list"]:
        f.write(segment_name.encode("ascii") + b"\0")
        num_frames = database["segments"][segment_name]["num_frames"]
        is_vowel = database["segments"][segment_name]["vowel"]
        f.write(struct.pack("<l", num_frames))
        f.write(struct.pack("<l", 1 if is_vowel else 0))
    f.write(b"\0")


def write_voice_file(f, database):
    write_voice_file_header(f, database)
    for segment_name in database["segments_list"]:
        array = database["segments"][segment_name]["frames"].flatten()
        packed_array = struct.pack(f"<{len(array)}h", *array)
        f.write(packed_array)


def read_string(f):
    result = []
    while True:
        c = f.read(1)
        if c == b"\0":
            break
        if len(result) > 255:
            raise ValueError("String longer than 255 characters")
        result.append(c)
    return b"".join(result).decode("ascii")


def read_voice_file_header(f, database):
    if f.read(len(MAGIC_WORD)) != MAGIC_WORD:
        raise RuntimeError("Invalid voice file")
    database["rate"] = struct.unpack("<l", f.read(4))[0]
    database["grain_length"] = struct.unpack("<l", f.read(4))[0]

    database["phonemes"] = []
    while True:
        phoneme = read_string(f)
        if len(phoneme) == 0:
            break
        database["phonemes"].append(phoneme)

    database["segments_list"] = []
    database["segments"] = {}
    while True:
        segment_id = read_string(f)
        if len(segment_id) == 0:
            break
        database["segments_list"].append(segment_id)
        database["segments"][segment_id] = {}
        database["segments"][segment_id]["num_frames"] = struct.unpack("<l", f.read(4))[
            0
        ]
        database["segments"][segment_id]["vowel"] = (
            struct.unpack("<l", f.read(4))[0] != 0
        )


def read_voice_file(f):
    database = {}
    read_voice_file_header(f, database)

    for segment_id in database["segments_list"]:
        num_frames = database["segments"][segment_id]["num_frames"]
        num_samples = num_frames * database["grain_length"]
        array = np.array(struct.unpack(f"<{num_samples}h", f.read(num_samples * 2)))
        array = array.reshape(num_frames, database["grain_length"])
        database["segments"][segment_id]["frames"] = array

    return database


def main():
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("in_dir")
    parser.add_argument("out_file")
    parser.add_argument("-l", "--log", default="info")
    args = parser.parse_args()

    log_map = {
        "debug": logging.DEBUG,
        "info": logging.INFO,
        "warning": logging.WARNING,
        "error": logging.ERROR,
        "critical": logging.CRITICAL,
    }
    logging.basicConfig(level=log_map[args.log])

    segment_database = CorpusAnalyzer(args.in_dir).render_database()

    with open(args.out_file, "wb") as f:
        write_voice_file(f, segment_database)


def locate_segment():
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("in_dir")
    parser.add_argument("segment")
    args = parser.parse_args()

    corpus_analyzer = CorpusAnalyzer(args.in_dir)
    segment_id = tuple(oddvoices.phonology.parse_pronunciation(args.segment))
    markers = corpus_analyzer.markers[segment_id]
    start_timestamp = seconds_to_timestamp(markers["start"] / corpus_analyzer.rate)
    end_timestamp = seconds_to_timestamp(markers["end"] / corpus_analyzer.rate)
    print(start_timestamp + " - " + end_timestamp)
