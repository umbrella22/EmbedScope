# EmbedScope mark

The observation aperture surrounds a diamond core. Two offset brackets and their
diagonal openings express looking inside a running device. The mark contains no
letters or initials.

The initial concept was generated on 2026-11-03 with `codex-image-gen-bridge`,
using model `gpt-image-2.5-sunburst` from the configured custom image provider.
The production artwork follows that generated silhouette, redrawn as three
flat SVG paths with consistent geometry and no generated background or texture.

| Asset | Use |
| --- | --- |
| `mark.svg` | Canonical mint mark, transparent, vector |
| `mark-light.svg` | Deep teal variant for light backgrounds |
| `mark.png` | 1024 px transparent mint export |
| `mark-mask.png` | 256 px white alpha mask, tinted by the native theme |
| `app-icon.svg`, `app-icon.png` | Ink tile with the mint mark and platform clear space |
| `app-icon.icns` | macOS app bundle, including Retina sizes |
| `app-icon.ico` | Windows icon asset, 16–256 px |

Keep at least one diamond radius of clear space around the mark. Preserve the
central diamond and both openings in small exports; do not add circuitry,
letters, gradients or shadows. On light surfaces use deep teal `#23856d`; on
dark surfaces use mint `#76dcc0`. Ink is `#171a1e`.

SVG is the editable source. The PNGs and platform icon files are checked-in
exports, so building the Inspector does not require an SVG renderer or external
asset service. When changing the silhouette, update both SVG compositions and
regenerate their raster exports before packaging.
