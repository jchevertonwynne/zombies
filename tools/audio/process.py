#!/usr/bin/env python3
"""Turn a recording into one variant file of an audio slot, and record where it came from.

    python3 tools/audio/process.py ~/Downloads/step.wav footstep_wood \\
        --group footsteps --source https://freesound.org/people/x/sounds/123/ --licence CC0 \\
        --start 0.12 --end 0.55 --name "Freesound 123, a step on boards"

What it does, with ffmpeg:

1. Trims to `--start`..`--end` (seconds; either may be left off), and fades the end out over
   `--fade-out` seconds if asked, so a short hard cut (a gunshot's tail) doesn't click.
2. Normalises loudness to the target of its cue group (`--group`, see GROUPS), so every file in a
   group sits at the same level whatever it was recorded at. Clips too short for a LUFS reading
   (a footstep is a few hundred ms) are matched on their mean level instead. Peaks stay under -1 dB.
3. Mixes down to mono for a Spatial cue (the slot's `Mix` in `src/audio/cues.rs`; the per-floor
   `footstep_<family>` slots are Spatial) and keeps stereo for a Flat one. `--mono` and `--stereo`
   override.
4. Encodes OGG Vorbis at quality 4, with no tags.
5. Writes `assets/audio/<slot>_<n>.ogg`, `n` being the next free variant number (or `--variant n`),
   and adds or replaces its entry in `assets/manifest.json`. Source recordings are never committed.

A slot's files are `<slot>.ogg` or `<slot>_1.ogg`, `<slot>_2.ogg`...: never both, and the numbers
have no gaps (see `src/audio/slots.rs`). The tool refuses to make a numbered variant beside a plain
file. Per-floor footsteps are the slots `footstep_wood`, `footstep_tile`, `footstep_brick`,
`footstep_concrete`, `footstep_metal`, `footstep_cobble` and `footstep_dirt`; a floor with no files
steps on the plain `footstep` slot.

Needs `ffmpeg` with libvorbis.
"""
import argparse
import datetime
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
AUDIO = ROOT / "assets" / "audio"
MANIFEST = ROOT / "assets" / "manifest.json"
CUES = ROOT / "src" / "audio" / "cues.rs"

# Integrated loudness targets in LUFS, per cue group. Quiet, frequent sounds sit low, so the mix
# is the cue's own `gain` on top of a consistent base.
GROUPS = {
    "footsteps": -30.0,
    "foley": -26.0,
    "weapons": -16.0,
    "impacts": -20.0,
    "vocals": -22.0,
    "ui": -20.0,
    "ambience": -28.0,
    "music": -20.0,
}
# What a clip too short to read in LUFS is matched on, mean level in dB, in the same order of loudness.
SHORT_MEAN_DB_OFFSET = 4.0
PEAK_CEILING_DB = -1.0
FLOOR_SLOTS = ("wood", "tile", "brick", "concrete", "metal", "cobble", "dirt")


def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True)


def slot_mix(slot):
    """'spatial' or 'flat' for a slot, from the cue table; None if it isn't a cue."""
    if slot in {f"footstep_{family}" for family in FLOOR_SLOTS}:
        return "spatial"
    leaf = slot.rsplit("/", 1)[-1]
    match = re.search(r'=> "%s",\s*\w+,\s*(Flat|Spatial),' % re.escape(leaf), CUES.read_text())
    return match.group(1).lower() if match else None


def measure(path, filters):
    """Integrated LUFS and mean volume of the trimmed clip, either None if unreadable."""
    lufs = None
    result = run(["ffmpeg", "-hide_banner", "-nostats", "-i", str(path), "-af", filters + "loudnorm=print_format=json", "-f", "null", "-"])
    match = re.search(r"\{[^{}]*\"input_i\"[^{}]*\}", result.stderr, re.S)
    if match:
        value = float(json.loads(match.group(0))["input_i"])
        if value > -70.0:
            lufs = value
    mean = None
    result = run(["ffmpeg", "-hide_banner", "-nostats", "-i", str(path), "-af", filters + "volumedetect", "-f", "null", "-"])
    match = re.search(r"mean_volume: (-?[\d.]+) dB", result.stderr)
    if match:
        mean = float(match.group(1))
    return lufs, mean


def next_variant(slot):
    plain = AUDIO / f"{slot}.ogg"
    if plain.exists():
        sys.exit(f"{plain} exists, so the slot is a single plain file: remove it to use numbered variants")
    taken = [int(m.group(1)) for p in AUDIO.glob(f"{slot}_*.ogg") if (m := re.fullmatch(re.escape(pathlib.Path(slot).name) + r"_(\d+)\.ogg", p.name))]
    return max(taken, default=0) + 1


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("input")
    parser.add_argument("slot", help="the slot name, e.g. footstep_wood (a subfolder slot: announcer/announcer_nuke)")
    parser.add_argument("--group", required=True, choices=sorted(GROUPS), help="the cue group whose loudness it joins")
    parser.add_argument("--source", required=True, help="where the recording came from (its own page)")
    parser.add_argument("--licence", default="CC0", help="its licence as its source states it")
    parser.add_argument("--name", help="a short name for the manifest")
    parser.add_argument("--notes")
    parser.add_argument("--archive", help="a capture of the source page, for per-asset or contradictory sources")
    parser.add_argument("--start", type=float)
    parser.add_argument("--end", type=float)
    parser.add_argument("--fade-in", type=float, default=0.004, help="seconds to fade the start in over (default 0.004; a gunshot's attack wants less)")
    parser.add_argument("--fade-out", type=float, help="seconds to fade the end of the trimmed clip out over")
    parser.add_argument("--variant", type=int, help="the variant number to write (default: the next free one)")
    parser.add_argument("--plain", action="store_true", help="write <slot>.ogg, a slot of one file")
    channels = parser.add_mutually_exclusive_group()
    channels.add_argument("--mono", action="store_true")
    channels.add_argument("--stereo", action="store_true")
    args = parser.parse_args()

    if "ffmpeg" not in run(["which", "ffmpeg"]).stdout:
        sys.exit("ffmpeg isn't installed")
    mix = slot_mix(args.slot)
    mono = args.mono or (not args.stereo and mix == "spatial")
    if mix is None and not (args.mono or args.stereo):
        print(f"note: {args.slot} isn't a declared cue, so it's kept stereo (pass --mono for a Spatial one)", file=sys.stderr)

    trim = ""
    if args.start is not None or args.end is not None:
        parts = []
        if args.start is not None:
            parts.append(f"start={args.start}")
        if args.end is not None:
            parts.append(f"end={args.end}")
        trim = f"atrim={':'.join(parts)},asetpts=PTS-STARTPTS,"
    # Short clips fade a few ms in so a hard cut doesn't click.
    fades = f"afade=t=in:d={args.fade_in},"
    # Reversed, so the fade lands on the end whatever the clip's length (and so the loudness
    # reading below sees it too).
    fade_out = f"areverse,afade=t=in:d={args.fade_out},areverse," if args.fade_out else ""
    pre = trim + ("pan=mono|c0=0.5*c0+0.5*c1," if mono else "") + fade_out

    lufs, mean = measure(args.input, pre)
    target = GROUPS[args.group]
    if lufs is not None:
        gain = target - lufs
    elif mean is not None:
        gain = (target - SHORT_MEAN_DB_OFFSET) - mean
    else:
        sys.exit("couldn't measure the clip: is the input readable audio?")

    AUDIO.mkdir(parents=True, exist_ok=True)
    if args.plain:
        out = AUDIO / f"{args.slot}.ogg"
        if any(AUDIO.glob(f"{args.slot}_[0-9]*.ogg")):
            sys.exit(f"{args.slot} has numbered variants already: drop --plain")
    else:
        number = args.variant or next_variant(args.slot)
        out = AUDIO / f"{args.slot}_{number}.ogg"
    out.parent.mkdir(parents=True, exist_ok=True)

    # `latency=1` takes out the limiter's own look-ahead delay (5 ms by default), which would
    # otherwise push every attack that late.
    chain = f"{pre}{fades}volume={gain:.2f}dB,alimiter=limit={10 ** (PEAK_CEILING_DB / 20):.4f}:level=disabled:latency=1"
    result = run(["ffmpeg", "-hide_banner", "-nostats", "-y", "-i", args.input, "-af", chain, "-ar", "44100", "-c:a", "libvorbis", "-q:a", "4", "-map_metadata", "-1", str(out)])
    if result.returncode != 0:
        sys.exit(result.stderr)

    rel = out.relative_to(ROOT / "assets").as_posix()
    manifest = json.loads(MANIFEST.read_text())
    entry = {
        "path": rel,
        "name": args.name or f"{args.slot} recording",
        "source": args.source,
        "licence": args.licence,
        "fetched": datetime.date.today().isoformat(),
    }
    if args.archive:
        entry["archive"] = args.archive
    entry["notes"] = args.notes or (
        f"Trimmed, normalised to {target:g} LUFS ({args.group}), {'mono' if mono else 'stereo'}, OGG q4, via tools/audio/process.py"
    )
    manifest["assets"] = [e for e in manifest["assets"] if e["path"] != rel] + [entry]
    manifest["assets"].sort(key=lambda e: e["path"])
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"wrote {rel} ({out.stat().st_size} bytes, gain {gain:+.1f} dB, {'mono' if mono else 'stereo'}) and its manifest entry")


main()
