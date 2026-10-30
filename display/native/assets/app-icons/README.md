# Official application icons

These are unchanged upstream application identity assets, used only when a known
application's installed desktop icon cannot be resolved. An icon does not mean
the application is installed, running, trusted, operable or endorsed by ArchieOS.
Availability and Policy remain separate AppCatalog fields.

`manifest.json` records the exact project repository, immutable commit, source
path, HTTPS URL, SHA256, byte count, license basis and attribution for every file.
`licenses/` preserves the downloaded upstream notices and attribution evidence.
The listed names and logos remain the marks of their respective owners. No logo
was recolored, redrawn, generated, cropped or converted. Inkscape uses its official
flat branding variant to avoid renderer-dependent SVG filters and clipping.
Firefox is Mozilla's **Firefox Browser** logo, not the Firefox family mark.

| Project | Artwork license / attribution basis |
| --- | --- |
| KiCad | CC-BY-SA-4.0 in upstream bitmap SVG metadata; KiCad project |
| Firefox Browser | MPL-2.0 repository license; Mozilla trademark guidelines still apply |
| Kdenlive | Upstream GPL-3.0-or-later COPYING; KDE Kdenlive team |
| FreeCAD | Repository LGPL-2.1-or-later default; no separate SPDX notice in the SVG |
| Blender | Upstream GPL-2.0-or-later source distribution; Blender Foundation trademark retained |
| Inkscape | CC-BY-SA-3.0 metadata in upstream branding source; Andy / Andrew Michael Fitzsimon, Fitzsimon IT Consulting Pty Ltd |
| GIMP | CC-BY-SA-4.0 upstream logo guide; Aryeom Han and GIMP team |
| OBS Studio | GPL-2.0-or-later repository README/COPYING; OBS Project |
| LibreOffice | Repository MPL-2.0 / LGPL-3.0-or-later defaults; The Document Foundation trademark retained |

Repository-default licenses are identified as such rather than asserted to be a
separate icon-specific grant. Source and license links are recorded for review.
For the Creative Commons assets, the license texts are available at
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) and
[CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/).
Mozilla's [trademark policy](https://www.mozilla.org/en-US/foundation/trademarks/policy/)
and Blender's [logo guidance](https://www.blender.org/about/logo/) remain applicable.

## Reproduce or verify

From the repository root, using Python 3.10 or newer:

```sh
python scripts/fetch-app-icons.py --verify
python scripts/fetch-app-icons.py --fetch
python scripts/test_fetch_app_icons.py
```

Verification is offline and is the default. Fetching requests only the reviewed
commit-pinned HTTPS URLs, checks SHA256 and static image structure, then replaces
each matching file atomically. It never queries a latest version, installs an
application, starts a service or downloads during application startup. Both the
standard TLS verifier and optional native `truststore` path validate certificates.

QML continues using `app.icon`, either an installed `file:` URL or
`qrc:/archie/assets/app-icons/<file>`. `app.iconSource` reports `installed` or
`official_bundle`; it is provenance metadata, not another image URL.
