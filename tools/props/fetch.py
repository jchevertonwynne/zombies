"""Downloads for tools/props: Poly Haven glTF models and the CC0 fabric. Stdlib only."""

import json
import pathlib
import urllib.request

API = "https://api.polyhaven.com"
USER_AGENT = "zombies-props/1"


def get(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=120) as response:
        return response.read()


def get_json(url):
    return json.loads(get(url))


def polyhaven_model(asset_id, workdir, res="1k"):
    """Downloads a Poly Haven model's glTF, bin and textures into `workdir/asset_id`.
    Returns that directory and the asset's info."""
    info = get_json(f"{API}/info/{asset_id}")
    gltf = get_json(f"{API}/files/{asset_id}")["gltf"][res]["gltf"]
    out = pathlib.Path(workdir) / asset_id
    out.mkdir(parents=True, exist_ok=True)
    (out / pathlib.Path(gltf["url"]).name).write_bytes(get(gltf["url"]))
    for rel, entry in gltf["include"].items():
        path = out / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(get(entry["url"]))
    return out, info


def polyhaven_maps(asset_id, workdir, res="1k"):
    """Downloads a Poly Haven texture's Diffuse, nor_gl and arm PNGs. Returns
    {color, normal, orm} paths and the asset's info."""
    info = get_json(f"{API}/info/{asset_id}")
    files = get_json(f"{API}/files/{asset_id}")
    maps = {}
    for key, name in (("Diffuse", "color"), ("nor_gl", "normal"), ("arm", "orm")):
        if key == "Diffuse" and key not in files:
            key = next(k for k in ("col_1", "col_01") if k in files)
        url = files[key][res]["png"]["url"]
        path = pathlib.Path(workdir) / f"{asset_id}_{name}.png"
        path.write_bytes(get(url))
        maps[name] = path
    return maps, info
