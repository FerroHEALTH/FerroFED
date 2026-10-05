<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# FerroFED brand

FerroFED follows the FerroHEALTH family system and takes its own mark and its
own hue, as every product in the family does. The file set, the naming and the
variants are shared with the family; the mark and the palette are FerroFED's
own. Everything here is under the Business Source License 1.1 with the rest of
the repository.

## The mark

One query arrives at the gateway and fans out to three sites. The gateway takes the full hue; the sites belong to other organisations and take the light value.

## Palette, "Azure & Iron"

| Token | Hex | Use |
|---|---|---|
| azure | `#0369A1` | primary mark and accents; text on light |
| azure-light | `#7DD3FC` | the same voice on a dark ground; highlights in the mark |
| ink | `#0F172A` | text on light |
| mist | `#F1F5F9` | text on dark |
| tile | `#0B1020` | dark tile background |
| surface | `#F8FAFC` | light surface background |

The hue was chosen by measurement against the hues the family already owns:
worst-case CIEDE2000 distance over both grounds and normal, protanope and
deuteranope vision. The numbers and the bar are recorded in the family
repository's `assets/brand/README.md`, and the family site copies the two hue
values from `tokens.css` here verbatim.

## Files

| File | What it is |
|---|---|
| `ferrofed-icon.svg` | primary icon, full colour, transparent background, 64-unit viewBox at a 512 intrinsic size |
| `tokens.css` | the palette as CSS custom properties |
| `favicon.svg` | the browser-tab icon, byte-identical to `ferrofed-icon.svg` |
| `favicon-32.png` | the 32-pixel raster of `favicon.svg` |
| `favicon.ico` | the 16 and 32 pixel rasters in one file, for browsers that ask for `/favicon.ico` |

The lockups and the social card follow when the site needs them.

## Regenerating the favicon set

The book theme carries copies of `favicon.svg` and `favicon-32.png` under
`website/book/theme/`, and `scripts/checks/favicon-sync.sh` fails when a copy
drifts from its source. After changing the mark, run from the repository root:

```sh
cp assets/brand/ferrofed-icon.svg assets/brand/favicon.svg
rsvg-convert -w 32 -h 32 assets/brand/favicon.svg -o assets/brand/favicon-32.png
rsvg-convert -w 16 -h 16 assets/brand/favicon.svg -o /tmp/favicon-16.png
magick /tmp/favicon-16.png assets/brand/favicon-32.png assets/brand/favicon.ico
cp assets/brand/favicon.svg website/book/theme/favicon.svg
cp assets/brand/favicon-32.png website/book/theme/favicon.png
```
