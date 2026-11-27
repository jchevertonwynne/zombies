#!/usr/bin/env python3
"""Build the Archive's sound files (#542) from CC0 Freesound previews, through `process.py`.

    python3 tools/audio/archive_sounds.py DOWNLOADS [--only SLOT ...] [--keep DIR]

`DOWNLOADS` holds one `<freesound id>.mp3` per source (the HQ preview each page offers, after the
page's own CC0 link has been read; the ids are in SOURCES below). Nothing here runs a downloaded file:
ffmpeg only decodes them. Each slot is cut, looped, layered or convolved into a WAV in a temporary
folder, and `process.py` normalises it, encodes the OGG variant and writes its manifest entry.

The recipes follow #515 section 10, with these differences, each because there was no way to get the
file or no recording of the thing:
- there is no recording of a shelf on rails: `range_roll` is a train rumble low-passed and pitched down
  five semitones, under a stone-slab scrape and the wagon's wheels, and `range_thud` is a heavy-book thud
  with a stone tail and a book dropped on a table (for the wood hit), run through a cathedral impulse
  response with `afir`;
- Kenney's Impact Sounds are only offered through itch.io, with no direct download, so the wood hit
  and the metal click they were to give are made from the Freesound clips alone.

Needs `ffmpeg` with libvorbis, and `afir`.
"""
import argparse
import pathlib
import re
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
PROCESS = HERE / "process.py"

# id -> (author, title, page)
SOURCES = {
    849796: ("IENBA", "Low Old Wagon Metal Rattles"),
    591883: ("FunWithSound", "Train Passing By 15 Pitch Change Deep Rumble with Horn"),
    844329: ("NahuelMartinez", "Stone Slab Door Grinding - Heavy Rock Scrape (Mono)"),
    648959: ("IENBA", "Heavy Book"),
    743262: ("qubodup", "Stone Sliding"),
    836521: ("Valerie-Vivegnis", "Book dropped on table"),
    648945: ("Nox_Sound", "IR_Cathedral_5m_Stereo"),
    456814: ("Breviceps", "Creaking Wood"),
    635727: ("kyles", "room tone medium quiet library carpeted empty with heavy building ambience +distant sounds"),
    553080: ("Nox_Sound", "Ambiance_Atmosphere_Cave_Loop_Stereo"),
    177958: ("Sclolex", "Water Dripping in Cave"),
    162116: ("editor_adp", "Water Dripping (Large Echo)"),
    607071: ("radioscoopfi", "Water droplets falling in a cave"),
    856497: ("xkeril", "Turning page (heavy paper)"),
    382651: ("bbrocer", "Paper Page Turn"),
    656546: ("IENBA", "Page Turn"),
    632220: ("golovlev.sound", "paper, thin, tracing, blueprint, rustle"),
    506665: ("Rudmer_Rotteveel", "Wood Creak Single V10"),
    572645: ("sthran", "Wood Floor Creak"),
    667384: ("MasterSuite", "book pullout"),
}


def page(sound):
    return f"https://freesound.org/people/{SOURCES[sound][0]}/sounds/{sound}/"


def label(sound):
    return f"Freesound {sound}, {SOURCES[sound][1]} ({SOURCES[sound][0]})"


def ffmpeg(*args):
    result = subprocess.run(["ffmpeg", "-hide_banner", "-nostats", "-y", *args], capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(result.stderr[-1500:])


def mean_db(path):
    result = subprocess.run(["ffmpeg", "-hide_banner", "-nostats", "-i", str(path), "-af", "volumedetect", "-f", "null", "-"], capture_output=True, text=True)
    match = re.search(r"mean_volume: (-?[\d.]+) dB", result.stderr)
    return float(match.group(1)) if match else -40.0


class Work:
    def __init__(self, downloads, scratch):
        self.downloads = pathlib.Path(downloads)
        self.scratch = pathlib.Path(scratch)
        self.count = 0

    def source(self, sound):
        path = self.downloads / f"{sound}.mp3"
        if not path.is_file():
            sys.exit(f"{path} is missing")
        return path

    def out(self, name):
        self.count += 1
        return self.scratch / f"{self.count:02}_{name}.wav"

    def cut(self, sound, start, end, filters="", name="cut"):
        """`sound` from `start` to `end` seconds as a 44.1 kHz mono WAV, with `filters` applied after the trim."""
        out = self.out(name)
        chain = f"atrim=start={start}:end={end},asetpts=PTS-STARTPTS,pan=mono|c0=0.5*c0+0.5*c1" + (f",{filters}" if filters else "")
        ffmpeg("-i", str(self.source(sound)), "-af", chain, "-ar", "44100", str(out))
        return out

    def loop(self, src, start, length, fade, name="loop", filters=""):
        """A seamless loop `length` seconds long from `src`: the `fade` seconds past the end are crossfaded (equal power)
        into the start, so playing the file round again is continuous."""
        out = self.out(name)
        graph = (
            f"[0:a]atrim=start={start}:end={start + length + fade},asetpts=PTS-STARTPTS,asplit=3[a][b][c];"
            f"[a]atrim=0:{fade},asetpts=PTS-STARTPTS,afade=t=in:d={fade}:curve=qsin[h];"
            f"[b]atrim={length}:{length + fade},asetpts=PTS-STARTPTS,afade=t=out:d={fade}:curve=qsin[t];"
            f"[h][t]amix=inputs=2:normalize=0:duration=longest[cf];"
            f"[c]atrim={fade}:{length},asetpts=PTS-STARTPTS[body];"
            f"[cf][body]concat=n=2:v=0:a=1" + (f",{filters}" if filters else "")
        )
        ffmpeg("-i", str(src), "-filter_complex", graph, "-ar", "44100", str(out))
        return out

    def trim(self, src, start, end, fade_out=0.0, filters="", name="trim"):
        out = self.out(name)
        chain = f"atrim=start={start}:end={end},asetpts=PTS-STARTPTS"
        if fade_out:
            chain += f",afade=t=out:st={end - start - fade_out}:d={fade_out}"
        if filters:
            chain += f",{filters}"
        ffmpeg("-i", str(src), "-af", chain, "-ar", "44100", str(out))
        return out

    def mix(self, layers, name="mix"):
        """Sums `(path, relative dB)` layers, each first levelled to the same mean volume (so the dB is relative to the others)."""
        out = self.out(name)
        inputs, chains, tags = [], [], []
        for i, (path, db) in enumerate(layers):
            inputs += ["-i", str(path)]
            chains.append(f"[{i}:a]volume={-24.0 - mean_db(path) + db:.2f}dB[l{i}]")
            tags.append(f"[l{i}]")
        graph = ";".join(chains) + f";{''.join(tags)}amix=inputs={len(layers)}:normalize=0:duration=longest"
        ffmpeg(*inputs, "-filter_complex", graph, "-ar", "44100", str(out))
        return out

    def delayed(self, path, secs, name="delay"):
        out = self.out(name)
        ffmpeg("-i", str(path), "-af", f"adelay={int(secs * 1000)}:all=1", str(out))
        return out

    def room(self, dry, ir, wet_db=-7.0, tail=2.4, name="room"):
        """`dry` with the impulse response `ir` convolved in (ffmpeg afir), the wet leveled to `wet_db` under the dry's mean volume."""
        wet = self.out(name + "_wet")
        ffmpeg("-i", str(dry), "-i", str(ir), "-filter_complex", "[0:a]apad=pad_dur=3[d];[d][1:a]afir=dry=1:wet=1", "-ar", "44100", "-c:a", "pcm_f32le", str(wet))
        out = self.out(name)
        gain = mean_db(dry) + wet_db - mean_db(wet)
        graph = (
            f"[0:a]apad=pad_dur=3[d];[1:a]volume={gain:.2f}dB[w];[d][w]amix=inputs=2:normalize=0:duration=longest"
            f",atrim=0:{tail},asetpts=PTS-STARTPTS,afade=t=out:st={tail - 0.8}:d=0.8"
        )
        ffmpeg("-i", str(dry), "-i", str(wet), "-filter_complex", graph, "-ar", "44100", str(out))
        return out


def process(wav, slot, group, sources, name, notes, mono=True, extra=()):
    extra = list(extra)
    if "--variant" not in extra:
        extra += ["--variant", "1"]
    args = [
        sys.executable, str(PROCESS), str(wav), slot, "--group", group,
        "--source", " ; ".join(page(s) for s in sources),
        "--licence", "CC0",
        "--name", name,
        "--notes", f"Freesound HQ preview (mp3), CC0 on each page (its link is creativecommons.org/publicdomain/zero/1.0/, read 2026-11-26); {notes}; normalised, {'mono' if mono else 'stereo'}, OGG q4 via tools/audio/process.py (recipe: tools/audio/archive_sounds.py)",
        "--mono" if mono else "--stereo",
        *extra,
    ]
    subprocess.run(args, check=True)


def build(work):
    """Every slot, in the order of #515 section 10: (slot, function)."""
    jobs = {}

    def job(slot):
        def register(fn):
            jobs[slot] = fn
            return fn

        return register

    @job("range_rattle")
    def _():
        wav = work.loop(work.source(849796), 2.0, 8.0, 1.0, "rattle", "highpass=f=70")
        process(wav, "range_rattle", "foley", [849796], label(849796) + ", chain-and-rattle section as a loop",
                "2.0-11.0 s cross-faded (1 s) into a seamless 8 s loop; the Range's Warning and Waiting")

    @job("range_roll")
    def _():
        # A train's deep rumble, low-passed and dropped five semitones (resampled slower), under the stone scrape and the wagon's wheels.
        rumble = work.cut(591883, 0.0, 17.0, "lowpass=f=260,asetrate=32000*0.7492,aresample=44100", "rumble")
        rumble = work.loop(rumble, 3.0, 6.0, 1.0, "rumble_loop")
        scrape = work.loop(work.source(844329), 5.2, 6.0, 1.0, "scrape_loop", "lowpass=f=1800")
        wheels = work.loop(work.source(849796), 14.0, 6.0, 1.0, "wheels_loop", "lowpass=f=2500")
        wav = work.mix([(rumble, 0.0), (scrape, -9.0), (wheels, -8.0)], "roll")
        process(wav, "range_roll", "foley", [591883, 844329, 849796],
                f"{label(591883)} low-passed 260 Hz and pitched down five semitones, under {label(844329)} (scrape, 5.2-12.2 s) and {label(849796)} (wheels, 14-21 s)",
                "three 6 s cross-faded loops layered into one seamless 6 s loop; the moving Bookcases' roll")

    @job("range_thud")
    def _():
        book = work.cut(648959, 1.40, 2.45, "", "book")
        stone = work.cut(743262, 0.0, 0.39, "volume=1.0", "stone")
        drop = work.cut(836521, 1.20, 2.20, "lowpass=f=900", "drop")
        for variant, speed in ((1, 1.0), (2, 0.9)):
            body = work.mix([(book, 0.0), (work.delayed(stone, 0.06), -7.0), (work.delayed(drop, 0.03), -5.0)], f"thud{variant}")
            body = work.trim(body, 0.0, 1.6, 0.2, f"lowpass=f=2200,asetrate=44100*{speed},aresample=44100", f"thud_cut{variant}")
            wav = work.room(body, work.cut(648945, 0.0, 2.6, "", "ir"), -8.0, 2.4, f"thud_room{variant}")
            process(wav, "range_thud", "impacts", [648959, 743262, 836521, 648945],
                    f"{label(648959)} (thud, 1.4-2.45 s) + {label(743262)} (stone tail) + {label(836521)} (a book dropped, for the wood hit), "
                    f"{'at natural speed' if speed == 1.0 else 'slowed to 0.9x'}, run through {label(648945)} offline with ffmpeg afir at -8 dB under the dry",
                    "a Range's Bookcases coming to Rest", extra=["--variant", str(variant)])

    @job("wheel_turn")
    def _():
        for variant, (start, end, speed) in enumerate(((0.10, 1.35, 1.15), (0.85, 2.10, 0.95)), 1):
            wav = work.cut(456814, start, end, f"asetrate=44100*{speed},aresample=44100,highpass=f=120", f"wheel{variant}")
            wav = work.trim(wav, 0.0, (end - start) / speed, 0.15, "", f"wheel_cut{variant}")
            process(wav, "wheel_turn", "foley", [456814], label(456814) + f", {start}-{end} s at {speed}x speed",
                    "a hand Wheel's creak as it is turned", extra=["--variant", str(variant)])

    @job("archive_tone")
    def _():
        wav = work.loop(work.source(635727), 10.0, 24.0, 3.0, "tone", "highpass=f=40")
        process(wav, "archive_tone", "ambience", [635727], label(635727),
                "10-37 s cross-faded (3 s) into a seamless 24 s loop; the reading and stack levels' room tone")

    @job("cellar_bed")
    def _():
        wav = work.loop(work.source(553080), 4.0, 40.0, 2.0, "bed")
        process(wav, "cellar_bed", "ambience", [553080], label(553080),
                "4-46 s cross-faded (2 s) into a seamless 40 s loop (its own loop point is lost to the preview's encoder padding); the five cellar Zones' bed")

    @job("drip")
    def _():
        picks = ((177958, 24.95, 26.35, 0.5), (162116, 19.95, 22.15, 0.9), (607071, 1.70, 3.40, 0.4))
        for variant, (sound, start, end, fade) in enumerate(picks, 1):
            wav = work.cut(sound, start, end, "", f"drip{variant}")
            wav = work.trim(wav, 0.0, end - start, fade, "", f"drip_cut{variant}")
            process(wav, "drip", "foley", [sound], label(sound) + f", {start}-{end} s",
                    "one drip falling in the Archive's cellar", extra=["--variant", str(variant)])

    @job("page_rustle")
    def _():
        picks = ((856497, 0.20, 1.20, ""), (382651, 2.50, 4.90, "highpass=f=200,afftdn=nr=15:nf=-55"), (656546, 2.70, 3.60, ""), (632220, 1.40, 3.40, ""))
        for variant, (sound, start, end, clean) in enumerate(picks, 1):
            wav = work.cut(sound, start, end, clean, f"page{variant}")
            wav = work.trim(wav, 0.0, end - start, 0.25, "", f"page_cut{variant}")
            process(wav, "page_rustle", "foley", [sound], label(sound) + f", {start}-{end} s",
                    "a phantom page turn from the stacks or the Reading Room" + ("; band-passed and denoised (afftdn), the recording is very quiet" if clean else ""), extra=["--variant", str(variant)])

    @job("shelf_creak")
    def _():
        picks = ((506665, 0.0, 1.19, 1.0), (572645, 0.0, 0.83, 1.0), (456814, 1.00, 2.19, 0.9))
        for variant, (sound, start, end, speed) in enumerate(picks, 1):
            wav = work.cut(sound, start, end, f"asetrate=44100*{speed},aresample=44100", f"creak{variant}")
            wav = work.trim(wav, 0.0, (end - start) / speed - 0.02, 0.12, "", f"creak_cut{variant}")
            process(wav, "shelf_creak", "foley", [sound], label(sound) + f", {start}-{end} s",
                    "a stack's shelf creaking, at a Roll's start or as a phantom", extra=["--variant", str(variant)])

    @job("book_fall")
    def _():
        wav = work.cut(836521, 1.15, 2.45, "", "fall1")
        wav = work.trim(wav, 0.0, 1.3, 0.3, "", "fall1_cut")
        process(wav, "book_fall", "impacts", [836521], label(836521) + ", 1.15-2.45 s", "a book falling at a Range's Rest", extra=["--variant", "1"])
        wav = work.cut(667384, 0.0, 0.36, "", "fall2")
        process(wav, "book_fall", "impacts", [667384], label(667384) + ", whole clip", "a book pulled from a shelf and falling", extra=["--variant", "2"])

    return jobs


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("downloads", help="a folder of <freesound id>.mp3 files")
    parser.add_argument("--only", nargs="*", help="build only these slots")
    parser.add_argument("--keep", help="keep the intermediate WAVs in this folder")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory() as scratch:
        scratch = args.keep or scratch
        pathlib.Path(scratch).mkdir(parents=True, exist_ok=True)
        jobs = build(Work(args.downloads, scratch))
        for slot, fn in jobs.items():
            if args.only and slot not in args.only:
                continue
            fn()


if __name__ == "__main__":
    main()
