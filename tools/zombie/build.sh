#!/usr/bin/env bash
# Rebuilds assets/enemies/zombie/ from its CC0 sources: fetches them into a work directory,
# installs MPFB into a throwaway Blender profile (nothing touches your own Blender setup), builds
# the three body glTFs and the clips glTF, makes the textures and turns them into KTX2.
#
#   tools/zombie/build.sh [work-dir]      (needs Blender 4.2+ on PATH or $BLENDER, curl, unzip, cargo)
#
# The sources, as recorded in assets/manifest.json:
#   MPFB 2.0.18        https://extensions.blender.org/add-ons/mpfb/
#   clothes and skin   https://files2.makehumancommunity.org/asset_packs/{shirts01,pants01,skins01}/
#   UAL2               https://opengameart.org/content/universal-animation-library-2
#   UAL1               https://opengameart.org/content/universal-animation-library
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
work="${1:-${TMPDIR:-/tmp}/zombies-zombie}"
blender="${BLENDER:-$(command -v blender || echo /Applications/Blender.app/Contents/MacOS/Blender)}"
out="$repo/assets/enemies/zombie"
mkdir -p "$work/packs" "$work/profile"

fetch() { [ -f "$2" ] || curl -fL --retry 3 -o "$2" "$1"; }

fetch "https://extensions.blender.org/download/sha256:c075b3e9b94b8e5556a5f03f4653602d02ea306de08126e8f55ed6dee28a2dad/add-on-mpfb-v2.0.18.zip" "$work/mpfb.zip"
for pack in shirts01 pants01 skins01; do
  fetch "https://files2.makehumancommunity.org/asset_packs/$pack/${pack}_cc0.zip" "$work/packs/$pack.zip"
  unzip -qo "$work/packs/$pack.zip" -d "$work/assets"
done
fetch "https://opengameart.org/sites/default/files/universal_animation_library_2standard.zip" "$work/ual2.zip"
unzip -qo "$work/ual2.zip" -d "$work/ual2"
fetch "https://opengameart.org/sites/default/files/universal_animation_librarystandard.zip" "$work/ual1.zip"
unzip -qo "$work/ual1.zip" -d "$work/ual1"
ual2="$work/ual2/Universal Animation Library 2 [Standard]/Unreal-Godot/UAL2_Standard.glb"
ual1="$work/ual1/Animation Library[Standard]/Godot/AnimationLibrary_Godot_Standard.glb"

export BLENDER_USER_RESOURCES="$work/profile"
"$blender" -b --command extension install-file -r user_default --enable "$work/mpfb.zip"

"$blender" -b --python "$here/build_clips.py" -- --ual1 "$ual1" --ual2 "$ual2" --out "$out/clips.glb"
for variant in labourer soldier civilian napalm shrieker sapper nova; do
  "$blender" -b --python "$here/build_bodies.py" -- --variant "$variant" --ual2 "$ual2" \
    --clothes "$work/assets" --out "$out/$variant.glb"
done

"$blender" -b --python "$here/make_textures.py" -- --packs "$work/assets" --out "$work/tex"
pack() { # name opacity-size
  (cd "$repo" && cargo run --release -q -p texpack -- --color "$work/tex/$1.png" --opacity "$work/tex/opaque$2.png" --out "assets/enemies/zombie/$1")
}
pack skin 2048
pack labourer_shirt 2048
pack labourer_pants 2048
for map in soldier_shirt soldier_pants civilian_shirt civilian_pants; do pack "$map" 1024; done
