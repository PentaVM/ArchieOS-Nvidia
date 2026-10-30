#!/usr/bin/env python3
"""Verify offline (default), or refetch the reviewed official icon bundle unchanged.

No application is installed and no URL/version is discovered at runtime. HTTPS,
SHA256 and static image checks run before replacing any bundled file.
"""
import argparse
import hashlib
import json
import pathlib
import re
import ssl
import struct
import tempfile
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
import zlib

DEFAULT_ROOT = pathlib.Path(__file__).resolve().parents[1] / "display/native/assets/app-icons"
MAX_BYTES = 2 * 1024 * 1024


def target_path(root, name):
    path = (root / name).resolve()
    if pathlib.Path(name).is_absolute() or not path.is_relative_to(root.resolve()):
        raise ValueError("asset path escapes bundle")
    return path


def validate_url(url):
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != "https" or parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ValueError("only pinned HTTPS source URLs are allowed")
    if parsed.hostname not in {"raw.githubusercontent.com", "gitlab.com"}:
        raise ValueError("source is outside the reviewed project hosts")
    if parsed.port not in {None, 443}:
        raise ValueError("source must use the standard HTTPS port")
    if not re.search(r"/[0-9a-f]{40}/", parsed.path):
        raise ValueError("source must include an immutable upstream commit")


class PinnedSourceRedirectHandler(urllib.request.HTTPRedirectHandler):
    """Reject an unsafe Location before urllib opens a follow-up connection."""

    def redirect_request(self, request, response, code, message, headers, new_url):
        validate_url(new_url)
        original = urllib.parse.urlsplit(request.full_url)
        redirected = urllib.parse.urlsplit(new_url)
        if redirected.hostname != original.hostname:
            raise ValueError("icon downloads cannot redirect to a different host")
        return super().redirect_request(request, response, code, message, headers, new_url)


def validate_image(name, blob):
    if name.endswith(".png"):
        if not blob.startswith(b"\x89PNG\r\n\x1a\n"):
            raise ValueError("not a PNG")
        pos, ended = 8, False
        while pos + 12 <= len(blob):
            length = struct.unpack(">I", blob[pos:pos+4])[0]
            chunk = blob[pos+4:pos+8]
            payload = blob[pos+8:pos+8+length]
            if pos+12+length > len(blob) or zlib.crc32(chunk+payload) != struct.unpack(">I", blob[pos+8+length:pos+12+length])[0]:
                raise ValueError("invalid PNG chunk")
            if chunk == b"IHDR":
                width, height = struct.unpack(">II", payload[:8])
                if not (0 < width <= 4096 and 0 < height <= 4096):
                    raise ValueError("unbounded PNG dimensions")
            pos += 12 + length
            if chunk == b"IEND":
                ended = pos == len(blob)
                break
        if not ended:
            raise ValueError("incomplete PNG")
        return
    if not name.endswith(".svg") or b"<!DOCTYPE" in blob.upper() or b"<!ENTITY" in blob.upper():
        raise ValueError("unsupported image or XML entities")
    root = ET.fromstring(blob)
    if root.tag != "{http://www.w3.org/2000/svg}svg":
        raise ValueError("not an SVG")
    blocked = {"script", "foreignObject", "image", "animate", "animateMotion", "animateTransform", "set", "a"}
    for node in root.iter():
        if node.tag.split("}")[-1] in blocked:
            raise ValueError("active or embedded SVG content")
        for key, value in node.attrib.items():
            local = key.split("}")[-1].lower()
            if local.startswith("on") or (local == "href" and not value.startswith("#")):
                raise ValueError("SVG event or external reference")
        for value in list(node.attrib.values()) + [node.text or ""]:
            if "@import" in value.lower() or any(not ref.strip(" \t\r\n'\"").startswith("#") for ref in re.findall(r"url\((.*?)\)", value, re.I)):
                raise ValueError("external SVG style reference")


def load_manifest(root):
    manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("schema") != 1:
        raise ValueError("unsupported icon manifest")
    return manifest


def verify_bytes(entry, blob):
    if len(blob) != entry["bytes"] or len(blob) > MAX_BYTES or hashlib.sha256(blob).hexdigest() != entry["sha256"]:
        raise ValueError("source bytes/hash do not match the reviewed manifest: " + entry["file"])
    if entry.get("kind") == "image":
        validate_image(entry["file"], blob)


def entries(manifest):
    return manifest["icons"] + manifest.get("documents", [])


def verify_bundle(root):
    manifest = load_manifest(root)
    for entry in entries(manifest):
        validate_url(entry["url"])
        verify_bytes(entry, target_path(root, entry["file"]).read_bytes())
    return len(manifest["icons"])


def fetch_bundle(root):
    # Windows deployments may optionally provide truststore to use the native CA
    # store. Both paths verify TLS; there is no insecure certificate bypass.
    try:
        import truststore
        context = truststore.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    except ImportError:
        context = ssl.create_default_context()
    opener = urllib.request.build_opener(
        PinnedSourceRedirectHandler(), urllib.request.HTTPSHandler(context=context))
    for entry in entries(load_manifest(root)):
        validate_url(entry["url"])
        request = urllib.request.Request(entry["url"], headers={"User-Agent": "ArchieOS-official-icon-fetch/1"})
        with opener.open(request, timeout=30) as response:
            validate_url(response.url)
            blob = response.read(MAX_BYTES + 1)
        verify_bytes(entry, blob)
        destination = target_path(root, entry["file"])
        destination.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as temporary:
            temporary.write(blob)
            staged = pathlib.Path(temporary.name)
        try:
            staged.replace(destination)
        finally:
            staged.unlink(missing_ok=True)
    return verify_bundle(root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    operation = parser.add_mutually_exclusive_group()
    operation.add_argument("--fetch", action="store_true", help="download the exact reviewed bytes again")
    operation.add_argument("--verify", action="store_true", help="verify locally without network (default)")
    parser.add_argument("--root", type=pathlib.Path, default=DEFAULT_ROOT)
    args = parser.parse_args()
    count = fetch_bundle(args.root) if args.fetch else verify_bundle(args.root)
    print(f"Verified {count} unchanged official application icons and their source/license documents")


if __name__ == "__main__":
    main()
