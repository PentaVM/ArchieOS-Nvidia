import importlib.util
import pathlib
import subprocess
import sys
import tempfile
import unittest
import urllib.request

SCRIPT = pathlib.Path(__file__).with_name("fetch-app-icons.py")
spec = importlib.util.spec_from_file_location("fetch_app_icons", SCRIPT)
icons = importlib.util.module_from_spec(spec)
if SCRIPT.exists():
    spec.loader.exec_module(icons)


class OfficialIconContracts(unittest.TestCase):
    def test_bundled_nine_icons_verify_without_network(self):
        self.assertTrue(hasattr(icons, "verify_bundle"), "missing offline icon verifier")
        manifest = icons.load_manifest(icons.DEFAULT_ROOT)
        self.assertEqual(len(manifest["icons"]), 9)
        self.assertEqual(icons.verify_bundle(icons.DEFAULT_ROOT), 9)
        self.assertEqual(len({entry["app_id"] for entry in manifest["icons"]}), 9)

    def test_svg_blocks_active_content_and_external_graphics(self):
        for fragment in [b'<script/>', b'<image href="https://example.com/a.png"/>',
                         b'<path onload="evil()"/>', b'<style>@import "a.css";</style>',
                         b'<path style="fill:url(https://example.com/a.svg)"/>',
                         b'<foreignObject/>', b'<animate/>']:
            with self.subTest(fragment=fragment):
                with self.assertRaises(ValueError):
                    icons.validate_image("test.svg", b'<svg xmlns="http://www.w3.org/2000/svg">'+fragment+b'</svg>')
        icons.validate_image("test.svg", b'<svg xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="a"/></defs><path fill="url(#a)"/></svg>')

    def test_tampering_and_path_escape_fail_before_overwrite(self):
        entry = {"file": "safe.svg", "sha256": "0"*64, "bytes": 6}
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(ValueError):
                icons.verify_bytes(entry, b"wrong!")
            with self.assertRaises(ValueError):
                icons.target_path(pathlib.Path(folder), "../escape.svg")

    def test_only_https_pinned_official_urls_are_accepted(self):
        for url in ["http://raw.githubusercontent.com/a/b/"+"a"*40+"/icon.svg",
                    "https://example.com/"+"a"*40+"/icon.svg",
                    "https://raw.githubusercontent.com/a/b/main/icon.svg"]:
            with self.subTest(url=url), self.assertRaises(ValueError):
                icons.validate_url(url)
        icons.validate_url("https://raw.githubusercontent.com/mozilla/protocol-assets/"+"a"*40+"/logos/firefox/browser/logo.svg")

    def test_redirects_are_checked_before_a_followup_request_is_created(self):
        source = "https://raw.githubusercontent.com/mozilla/protocol-assets/"+"a"*40+"/logos/firefox/browser/logo.svg"
        request = urllib.request.Request(source)
        handler = icons.PinnedSourceRedirectHandler()
        for target in [source.replace("https:", "http:"), "http://127.0.0.1/private",
                       "https://localhost/private", "https://example.com/"+"a"*40+"/icon.svg",
                       "https://gitlab.com/project/icons/-/raw/"+"a"*40+"/icon.svg",
                       source.replace("raw.githubusercontent.com/", "raw.githubusercontent.com:8080/"),
                       source.replace("/"+"a"*40+"/", "/main/")]:
            with self.subTest(target=target), self.assertRaises(ValueError):
                handler.redirect_request(request, None, 302, "Found", {}, target)
        # A same-host pinned redirect may produce a Request; it still performs no IO.
        redirected = handler.redirect_request(request, None, 302, "Found", {}, source)
        self.assertEqual(redirected.full_url, source)

    def test_fetch_and_verify_cannot_be_combined_into_an_accidental_download(self):
        result = subprocess.run([sys.executable, str(SCRIPT), "--fetch", "--verify"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("not allowed with argument", result.stderr)


if __name__ == "__main__":
    unittest.main()
