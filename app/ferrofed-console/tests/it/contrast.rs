// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The contrast of every text and ground pair the console paints with: the
//! roles of the `ferrohealth_ui` stylesheet with the console's brand theme
//! `style/theme.css` laid over them, as the page loads them, in the light
//! theme and the dark one, computed from the custom properties as WCAG 2.2
//! defines it: 4.5:1 for text (SC 1.4.3) and 3:1 for the edge of a control
//! and the focus ring (SC 1.4.11).
//!
//! A translucent ground is composited over each opaque ground and the worst
//! of them counts, so a tint is measured wherever a view can place it.
//! <https://www.w3.org/TR/WCAG22/#dfn-contrast-ratio>,
//! <https://www.w3.org/TR/WCAG22/#dfn-relative-luminance>

use std::collections::BTreeMap;

/// The roles the console paints with, from the library's stylesheet.
const STYLESHEET: &str = ferrohealth_ui::style::STYLESHEET;

/// The console's brand theme, which loads after the stylesheet.
const BRAND: &str = include_str!("../../style/theme.css");

/// The text minimum of WCAG 2.2 SC 1.4.3.
const TEXT: f64 = 4.5;

/// The non-text minimum of WCAG 2.2 SC 1.4.11.
const EDGE: f64 = 3.0;

/// The opaque grounds a view paints a region with.
const OPAQUE: &[&str] = &[
    "surface",
    "surface-raised",
    "surface-sunken",
    "surface-head",
    "neutral-ground",
];

/// The translucent or tinted grounds, laid over an opaque one.
const TINTED: &[&str] = &["accent-tint", "ok-ground", "warn-ground", "bad-ground"];

/// Every ground a body text can sit on.
const EVERY_GROUND: &[&str] = &[
    "surface",
    "surface-raised",
    "surface-sunken",
    "surface-head",
    "neutral-ground",
    "accent-tint",
    "ok-ground",
    "warn-ground",
    "bad-ground",
];

/// The opaque grounds and the accent's tint, where the accent is drawn.
const ACCENT_GROUNDS: &[&str] = &[
    "surface",
    "surface-raised",
    "surface-sunken",
    "surface-head",
    "accent-tint",
];

/// The grounds a highlighted token is drawn on.
const TOKEN_GROUNDS: &[&str] = &["surface-sunken", "accent-tint"];

/// Each text role and the grounds it can sit on.
const TEXT_PAIRS: &[(&str, &[&str])] = &[
    ("ink", EVERY_GROUND),
    ("muted", EVERY_GROUND),
    ("accent", ACCENT_GROUNDS),
    ("accent-hover", ACCENT_GROUNDS),
    ("accent-ink", &["accent", "accent-hover"]),
    (
        "neutral",
        &[
            "surface",
            "surface-raised",
            "surface-sunken",
            "surface-head",
            "neutral-ground",
        ],
    ),
    ("ok", &["ok-ground"]),
    ("warn", &["warn-ground"]),
    ("bad", &["bad-ground"]),
    ("syntax-keyword", TOKEN_GROUNDS),
    ("syntax-string", TOKEN_GROUNDS),
    ("syntax-number", TOKEN_GROUNDS),
    ("syntax-comment", TOKEN_GROUNDS),
    ("syntax-key", TOKEN_GROUNDS),
];

/// Each control edge (a pressed segment's border, the focus ring) and the
/// grounds it is drawn against.
const EDGE_PAIRS: &[(&str, &[&str])] = &[
    (
        "edge-control",
        &[
            "surface",
            "surface-raised",
            "surface-sunken",
            "surface-head",
        ],
    ),
    ("accent", ACCENT_GROUNDS),
    ("focus", ACCENT_GROUNDS),
];

/// An sRGB colour with its alpha, each channel in 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rgba {
    r: f64,
    g: f64,
    b: f64,
    a: f64,
}

impl Rgba {
    /// This colour laid over an opaque `ground`, blended in sRGB as a
    /// browser composites it.
    fn over(self, ground: Self) -> Self {
        let mix = |top: f64, under: f64| top * self.a + under * (1.0 - self.a);
        Self {
            r: mix(self.r, ground.r),
            g: mix(self.g, ground.g),
            b: mix(self.b, ground.b),
            a: 1.0,
        }
    }

    /// The relative luminance WCAG 2.2 defines.
    fn luminance(self) -> f64 {
        let linear = |c: f64| {
            let s = c / 255.0;
            if s <= 0.040_45 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }
}

/// The contrast ratio of two opaque colours.
fn ratio(one: Rgba, two: Rgba) -> f64 {
    let (a, b) = (one.luminance(), two.luminance());
    let (light, dark) = if a >= b { (a, b) } else { (b, a) };
    (light + 0.05) / (dark + 0.05)
}

/// `css` with every comment removed.
fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some((before, after)) = rest.split_once("/*") {
        out.push_str(before);
        rest = after.split_once("*/").map_or("", |(_, tail)| tail);
    }
    out.push_str(rest);
    out
}

/// The bodies of every rule whose selector is `selector`, each up to its
/// closing brace (a block of custom properties nests none), joined as one.
fn block(css: &str, selector: &str) -> String {
    let header = format!("{selector} {{");
    let bodies: Vec<&str> = css
        .split(&header)
        .skip(1)
        .map(|body| body.split_once('}').expect("the block should close").0)
        .collect();
    assert!(
        !bodies.is_empty(),
        "the stylesheet should carry `{selector}`"
    );
    bodies.join(";")
}

/// The colour `value` writes, `#rrggbb` or `rgb(r g b / a)`; `None` for a
/// value that is no colour.
fn parse_colour(value: &str) -> Option<Rgba> {
    if let Some(hex) = value.strip_prefix('#') {
        assert_eq!(hex.len(), 6, "a colour token is written #rrggbb: {value}");
        let channel = |at: usize| {
            let digits = hex.get(at..at + 2).expect("two hex digits");
            f64::from(u8::from_str_radix(digits, 16).expect("a hex channel"))
        };
        return Some(Rgba {
            r: channel(0),
            g: channel(2),
            b: channel(4),
            a: 1.0,
        });
    }
    let inner = value.strip_prefix("rgb(")?.strip_suffix(')')?;
    let (channels, alpha) = inner.split_once('/').unwrap_or((inner, "1"));
    let channels: Vec<f64> = channels
        .split_whitespace()
        .map(|c| c.parse().expect("an rgb() channel"))
        .collect();
    assert_eq!(channels.len(), 3, "rgb() has three channels: {value}");
    Some(Rgba {
        r: channels[0],
        g: channels[1],
        b: channels[2],
        a: alpha.trim().parse().expect("an rgb() alpha"),
    })
}

/// Every colour custom property of one theme block.
fn theme(body: &str) -> BTreeMap<String, Rgba> {
    body.split(';')
        .filter_map(|declaration| {
            let (name, value) = declaration.trim().split_once(':')?;
            let name = name.trim().strip_prefix("--")?;
            Some((name.to_owned(), parse_colour(value.trim())?))
        })
        .collect()
}

/// The colour `--name` of `theme`.
fn token(theme: &BTreeMap<String, Rgba>, name: &str) -> Rgba {
    *theme
        .get(name)
        .unwrap_or_else(|| panic!("the theme should define --{name}"))
}

/// The worst ratio of `fg` against `ground`, a tinted ground laid over each
/// opaque one in turn.
fn worst(theme: &BTreeMap<String, Rgba>, fg: &str, ground: &str) -> f64 {
    let fg = token(theme, fg);
    let ground_colour = token(theme, ground);
    let unders: Vec<Rgba> = if TINTED.contains(&ground) {
        OPAQUE
            .iter()
            .map(|under| ground_colour.over(token(theme, under)))
            .collect()
    } else {
        assert!(
            (ground_colour.a - 1.0).abs() < f64::EPSILON,
            "--{ground} is translucent and should be listed as a tinted ground"
        );
        vec![ground_colour]
    };
    unders
        .into_iter()
        .map(|under| ratio(fg.over(under), under))
        .fold(f64::INFINITY, f64::min)
}

/// The three theme blocks: light, the chosen dark, and the system's dark,
/// each the stylesheet's roles with the brand's block laid over them.
fn themes() -> Vec<(&'static str, BTreeMap<String, Rgba>)> {
    let css = strip_comments(STYLESHEET);
    let brand = strip_comments(BRAND);
    [
        ("light", ":root"),
        ("dark", ":root[data-theme=\"dark\"]"),
        (
            "system dark",
            ":root:not([data-theme=\"light\"], [data-theme=\"dark\"])",
        ),
    ]
    .into_iter()
    .map(|(name, selector)| {
        let mut colours = theme(&block(&css, selector));
        colours.extend(theme(&block(&brand, selector)));
        (name, colours)
    })
    .collect()
}

/// The two dark blocks, the chosen one and the system one, carry the same
/// colours, so measuring one measures both.
#[test]
fn the_system_dark_theme_is_the_chosen_dark_theme() {
    let themes = themes();
    let [(_, light), (_, dark), (_, system)] = themes.as_slice() else {
        panic!("three theme blocks");
    };
    assert_eq!(dark, system);
    for role in light.keys() {
        assert!(
            dark.contains_key(role)
                || !TEXT_PAIRS
                    .iter()
                    .chain(EDGE_PAIRS)
                    .any(|(fg, grounds)| fg == role || grounds.contains(&role.as_str())),
            "the dark theme should define --{role}, which a pair measures"
        );
    }
}

/// WCAG 2.2 SC 1.4.3 and SC 1.4.11 over every pair, in every theme.
#[test]
fn every_text_and_ground_pair_meets_wcag_aa() {
    let mut failures = Vec::new();
    for (name, theme) in themes() {
        for (minimum, pairs) in [(TEXT, TEXT_PAIRS), (EDGE, EDGE_PAIRS)] {
            for (fg, grounds) in pairs {
                for ground in *grounds {
                    let measured = worst(&theme, fg, ground);
                    if measured < minimum {
                        failures.push(format!(
                            "{name}: --{fg} on --{ground} is {measured:.2}:1, under {minimum}:1"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The focus ring is the brand's \`--focus\`, the accent in every theme, which
/// the pairs above hold to 3:1 on every ground, and the top bar that stays
/// in view never covers what has the focus (WCAG 2.2 SC 2.4.11).
#[test]
fn the_focus_ring_is_the_accent_and_the_scroll_padding_clears_the_top_bar() {
    let css = strip_comments(STYLESHEET);
    let focus = block(&css, ":focus-visible");
    assert!(focus.contains("outline: 2px solid var(--focus)"), "{focus}");
    for (name, theme) in themes() {
        assert_eq!(
            token(&theme, "accent"),
            token(&theme, "focus"),
            "{name}: the focus ring is the accent"
        );
    }
    let html = block(&css, "html");
    assert!(
        html.contains("scroll-padding-top: calc(var(--bar) + var(--space-default))"),
        "{html}"
    );
    assert!(css.contains("--bar: 3.5rem"), "the top bar has its height");
}

/// The computation itself, against the two ratios WCAG fixes: black on white
/// is 21:1 and a colour on itself is 1:1.
#[test]
fn the_ratio_is_the_wcag_contrast_ratio() {
    let black = parse_colour("#000000").expect("black");
    let white = parse_colour("#ffffff").expect("white");
    assert!((ratio(black, white) - 21.0).abs() < 1e-9);
    assert!((ratio(white, white) - 1.0).abs() < 1e-9);
    let half = parse_colour("rgb(0 0 0 / 0.5)").expect("half black");
    let blended = half.over(white);
    assert!((blended.r - 127.5).abs() < 1e-9 && (blended.a - 1.0).abs() < f64::EPSILON);
}
