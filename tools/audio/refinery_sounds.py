#!/usr/bin/env python3
"""Build the Refinery's sound files (#575) from CC0 Freesound previews, through `process.py`.

    python3 tools/audio/refinery_sounds.py DOWNLOADS [--only SLOT ...] [--keep DIR]

`DOWNLOADS` holds one `<freesound id>.mp3` per source (the HQ preview each page offers, after the page's own CC0 link has been read;
the ids are in SOURCES below). Nothing here runs a downloaded file: ffmpeg only decodes them. It reuses the Archive's cutting, looping
and layering (`archive_sounds.Work`), then hands each slot to `process.py`, which normalises it, encodes the OGG variant and writes its
manifest entry.

The recipes follow #510 section 9, with these notes:
- `valve_turn` is the layered substitute (no CC0 recording of a heavy handwheel exists): a valve opening (ninoing), a metal creak
  (ingudios) and the opening squeak of a water valve (JazzyBay), each dropped three semitones, one 1.4 s file. It is NOT the
  scriotxstudios valve turns, which are CC BY 4.0. If it reads as cartoonish the fallback is the first two layers only.
- `flare_roar` is a propane burner (NickPeeters) dropped an octave and low-passed at 900 Hz, cross-faded into an 8 s loop. It is NOT the
  smidoid flare, which is CC BY-NC 4.0.
- `gas_hiss` is Jofae's air hiss run into the tail of sniperous's steam, cross-faded into a 4 s loop. Not PatrickLieberkind, visions68 or
  GoodListener (CC BY).
- `blaze_loop` and `blaze_ignite` are the files of `fire_pool` and `napalm_windup` under their own slots (copies, with their own entries).

Needs `ffmpeg` with libvorbis.
"""
import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

import archive_sounds as base

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
AUDIO = ROOT / "assets" / "audio"
MANIFEST = ROOT / "assets" / "manifest.json"

# id -> (author, title). Every licence was read on the sound's own page (creativecommons.org/publicdomain/zero/1.0/).
SOURCES = {
    367125: ("Jofae", "Air Hiss"),
    365071: ("sniperous", "Steam.wav"),
    698386: ("ninoing", "Valve Opening"),
    119467: ("ingudios", "Metal Creak 3.wav"),
    119468: ("ingudios", "Metal Creak 4.wav"),
    435052: ("JazzyBay", "Hose Sounds.mp3"),
    243763: ("NickPeeters", "gas vuur dakwerken gasbrander luchtballon.wav"),
    278987: ("fimrod", "Large Warehouse/Factory Ambience.wav"),
    512679: ("AudioPapkin", "Factory machine 003.wav"),
    271096: ("IanStarGem", "Industrial Factory/Fans Loop"),
    245945: ("sophiehall3535", "Metal creak.wav"),
    487837: ("BenDrain", "Metal_Pipe_04.wav"),
}
base.SOURCES.update(SOURCES)

READ = "2026-11-27"


def process(wav, slot, group, sources, name, notes, variant=1):
    args = [
        sys.executable, str(base.PROCESS), str(wav), slot, "--group", group,
        "--source", " ; ".join(base.page(s) for s in sources),
        "--licence", "CC0",
        "--name", name,
        "--notes", f"Freesound HQ preview (mp3), CC0 on each page (its link is creativecommons.org/publicdomain/zero/1.0/, read {READ}); {notes}; normalised, mono, OGG q4 via tools/audio/process.py (recipe: tools/audio/refinery_sounds.py)",
        "--mono", "--variant", str(variant),
    ]
    subprocess.run(args, check=True)


def level(work, path, name):
    """`path` levelled to the common mean volume, so layers and joins are matched."""
    return work.mix([(path, 0.0)], name)


def join(work, a, b, fade, name="join"):
    out = work.out(name)
    base.ffmpeg("-i", str(a), "-i", str(b), "-filter_complex", f"acrossfade=d={fade}:c1=qsin:c2=qsin", "-ar", "44100", str(out))
    return out


def copy_slot(slot, file, why):
    """`<slot>_1.ogg` as a copy of the existing `file`, with its own manifest entry saying so."""
    manifest = json.loads(MANIFEST.read_text())
    entries = manifest["assets"]
    source = next(e for e in entries if e["path"] == f"audio/{file}")
    shutil.copyfile(AUDIO / file, AUDIO / f"{slot}_1.ogg")
    entry = dict(source, path=f"audio/{slot}_1.ogg", notes=f"{source['notes']}; CC0 on each page (its link is creativecommons.org/publicdomain/zero/1.0/, re-read {READ}); the same file as {file}, a second slot for {why}")
    entries[:] = [e for e in entries if e["path"] != entry["path"]]
    entries.append(entry)
    entries.sort(key=lambda e: e["path"])
    MANIFEST.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


def build(work):
    jobs = {}

    def job(slot):
        def register(fn):
            jobs[slot] = fn
            return fn

        return register

    @job("blaze_loop")
    def _():
        copy_slot("blaze_loop", "fire_pool_1.ogg", "each Lit Blaze's crackle (the three nearest play)")

    @job("blaze_ignite")
    def _():
        copy_slot("blaze_ignite", "napalm_windup_1.ogg", "a Fuel Line lighting")

    @job("gas_hiss")
    def _():
        head = level(work, work.cut(367125, 0.3, 1.5, "highpass=f=300", "hiss_head"), "hiss_head_l")
        tail = level(work, work.cut(365071, 1.5, 5.6, "highpass=f=300", "hiss_tail"), "hiss_tail_l")
        wav = work.loop(join(work, head, tail, 0.35), 0.0, 4.0, 0.6, "hiss_loop")
        process(wav, "gas_hiss", "ambience", [367125, 365071], f"{base.label(367125)} (0.3-1.5 s) run into the tail of {base.label(365071)} (1.5-5.6 s), both high-passed at 300 Hz",
                "a 4 s loop (0.6 s equal-power cross-fade); the gas hissing from each Blaze and Valve through a Flare-up")

    @job("valve_turn")
    def _():
        drop = 0.8409  # three semitones down
        creak = work.cut(119467, 0.0, 1.9, f"highpass=f=150,asetrate=48000*{drop}", "creak")
        squeak = work.cut(435052, 1.1, 2.6, f"highpass=f=400,asetrate=48000*{drop}", "squeak")
        opening = work.cut(698386, 0.0, 0.65, f"asetrate=44100*{drop}", "opening")
        wav = work.mix([(creak, 0.0), (work.delayed(squeak, 0.05), -5.0), (work.delayed(opening, 0.45), -1.0)], "valve")
        wav = work.trim(wav, 0.0, 1.4, 0.2, "", "valve_cut")
        process(wav, "valve_turn", "foley", [698386, 119467, 435052],
                f"layered, each dropped three semitones: {base.label(119467)} (creak, 0-1.9 s) + {base.label(435052)} (the opening squeak, 1.1-2.6 s, -5 dB, 50 ms in) + {base.label(698386)} (valve opening, whole clip, 0.45 s in)",
                "a 1.4 s layered substitute for a heavy handwheel turn (no CC0 recording exists); a Valve is used")

    @job("flare_roar")
    def _():
        slowed = work.cut(243763, 0.5, 9.5, "asetrate=24000,lowpass=f=900", "roar")
        wav = work.loop(slowed, 2.0, 8.0, 1.5, "roar_loop")
        process(wav, "flare_roar", "ambience", [243763], f"{base.label(243763)}, 0.5-9.5 s dropped an octave and low-passed at 900 Hz",
                "an 8 s loop (1.5 s equal-power cross-fade), a substitute for a flare stack's roar (no CC0 recording exists); the flare, faint Map-wide")

    @job("refinery_hum")
    def _():
        wav = work.loop(work.cut(278987, 1.0, 19.0, "highpass=f=30", "hum"), 0.0, 14.0, 2.5, "hum_loop")
        process(wav, "refinery_hum", "ambience", [278987], f"{base.label(278987)}, 1-19 s",
                "a 14 s loop (2.5 s equal-power cross-fade); the bed under the whole Map")

    @job("pump_drone")
    def _():
        wav = work.loop(work.cut(512679, 0.5, 14.5, "highpass=f=40", "pump"), 0.0, 10.0, 1.5, "pump_loop")
        process(wav, "pump_drone", "ambience", [512679], f"{base.label(512679)}, 0.5-14.5 s",
                "a 10 s loop (1.5 s equal-power cross-fade); the Compressor House's pump sets while the Power is on")

    @job("control_fans")
    def _():
        wav = work.loop(work.cut(271096, 0.2, 6.4, "highpass=f=60", "fans"), 0.0, 5.0, 0.8, "fans_loop")
        process(wav, "control_fans", "ambience", [271096], f"{base.label(271096)}, 0.2-6.4 s",
                "a 5 s loop (0.8 s equal-power cross-fade); the Control House's fans")

    @job("steel_groan")
    def _():
        picks = ((119467, 0.1, 2.0, 1.0), (119468, 0.2, 3.0, 0.85), (245945, 0.25, 2.4, 1.0))
        for variant, (sound, start, end, speed) in enumerate(picks, 1):
            rate = 48000
            wav = work.cut(sound, start, end, f"asetrate={rate}*{speed}", f"groan{variant}")
            wav = work.trim(wav, 0.0, (end - start) / speed - 0.02, 0.3, "", f"groan_cut{variant}")
            process(wav, "steel_groan", "foley", [sound], f"{base.label(sound)}, {start}-{end} s" + (f" at {speed}x speed" if speed != 1.0 else ""),
                    "a phantom groan of a tank or catwalk's steel", variant=variant)

    @job("pipe_clank")
    def _():
        wav = work.cut(487837, 0.15, 2.4, "", "clank")
        wav = work.trim(wav, 0.0, 2.25, 0.5, "", "clank_cut")
        process(wav, "pipe_clank", "impacts", [487837], f"{base.label(487837)}, 0.15-2.4 s",
                "a Fuel Line shutting, and a phantom clank off the pipe rack")

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
        jobs = build(base.Work(args.downloads, scratch))
        for slot, fn in jobs.items():
            if args.only and slot not in args.only:
                continue
            fn()


if __name__ == "__main__":
    main()
