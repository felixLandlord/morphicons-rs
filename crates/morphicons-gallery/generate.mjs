// Generates src/data.rs: a curated icon set for the demos, extracted from the
// real npm packages (not hand-copied):
//   - Lucide      (ISC)  lucide-static/icon-nodes.json
//   - Heroicons   (MIT)  heroicons/24/outline/*.svg
//   - Tabler      (MIT)  @tabler/icons/tabler-nodes-outline.json
//
// Usage (from a folder where the three packages are unpacked or installed):
//   npm pack lucide-static heroicons @tabler/icons && for f in *.tgz; do mkdir -p "${f%.tgz}" && tar xzf "$f" -C "${f%.tgz}"; done
//   node crates/morphicons-gallery/generate.mjs <lucide-static dir> <heroicons dir> <@tabler/icons dir>

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const [lucideDir, heroDir, tablerDir] = process.argv.slice(2);
if (!tablerDir) {
  console.error("usage: node generate.mjs <lucide-static> <heroicons> <@tabler/icons>");
  process.exit(1);
}

const LUCIDE = `menu x check plus minus arrow-right arrow-down arrow-left arrow-up chevron-down
chevron-right square circle diamond triangle heart star bell settings camera sun moon search
play pause aperture house zap copy download upload trash pencil eye eye-off lock lock-open
key shield mail phone user users face-slightly-smiling calendar clock cloud wifi bluetooth battery bookmark
folder folder-open file-text clipboard printer save send share-2 link external-link paperclip
thumbs-up message-circle info circle-question-mark circle-alert triangle-alert ellipsis map-pin globe
compass flag refresh-cw volume-2 volume-x mic repeat shuffle music film image monitor
smartphone headphones funnel sliders-horizontal tag shopping-cart credit-card log-in log-out
terminal code git-branch bug database server cpu wrench package gift award trending-up
activity layers layout-grid list sparkles rocket flame palette feather scissors umbrella
anchor coffee`;

const HEROICONS = `bars-3 x-mark check plus minus arrow-right arrow-down home magnifying-glass user
bell heart star trash pencil-square envelope calendar clock folder bookmark
chat-bubble-oval-left hand-thumb-up map-pin globe-alt lock-closed eye arrow-down-tray
arrow-up-tray share link phone photo play pause musical-note funnel credit-card shopping-cart
gift bolt sun moon cloud camera cog-6-tooth scissors wifi`;

const TABLER = `sparkles flame rocket flask bulb puzzle school key shield-check mood-smile send
paperclip terminal-2 code hourglass cake files check x eye-off player-pause folder-open
volume volume-off home search settings user bell heart star trash pencil download upload
share link phone photo music filter map-pin gift database cpu wifi coffee droplet mood-happy
briefcase diamond crown anchor feather umbrella snowflake apple eye paw pyramid plant skull
brush wand robot meteor ufo cat ghost planet bolt brand-github bike moon-stars copy sun moon
cloud camera scissors player-play`;

const names = (s) => s.split(/\s+/).filter(Boolean);
const GEOMETRY = new Set(["d", "x", "y", "x1", "y1", "x2", "y2", "cx", "cy", "r", "rx", "ry", "width", "height", "points"]);
const TAGS = new Set(["path", "line", "circle", "ellipse", "rect", "polyline", "polygon"]);

// Keep only drawable stroke primitives and their geometry attributes.
const clean = (nodes, where) =>
  nodes
    .filter(([tag, attrs]) => {
      if (!TAGS.has(tag)) throw new Error(`${where}: unsupported <${tag}>`);
      return attrs.stroke !== "none"; // Tabler's invisible bounding box
    })
    .map(([tag, attrs]) => [
      tag,
      Object.entries(attrs)
        .filter(([k]) => GEOMETRY.has(k))
        .map(([k, v]) => [k, String(v)]),
    ]);

// Heroicons ship flat SVG files: a regex over self-closing elements is enough.
const parseSvg = (src) =>
  [...src.matchAll(/<([a-z]+)((?:\s+[\w-]+="[^"]*")*)\s*\/>/g)].map((m) => [
    m[1],
    Object.fromEntries([...m[2].matchAll(/([\w-]+)="([^"]*)"/g)].map((a) => [a[1], a[2]])),
  ]);

const lucide = JSON.parse(readFileSync(join(lucideDir, "icon-nodes.json"), "utf8"));
const tabler = JSON.parse(readFileSync(join(tablerDir, "tabler-nodes-outline.json"), "utf8"));
const version = (dir) => JSON.parse(readFileSync(join(dir, "package.json"), "utf8")).version;

const entries = [];
const missing = [];
const add = (lib, name, nodes) => {
  if (!nodes) return missing.push(`${lib}:${name}`);
  entries.push({ lib, name, nodes: clean(nodes, `${lib}:${name}`) });
};
for (const n of names(LUCIDE)) add("Lucide", n, lucide[n]);
for (const n of names(HEROICONS)) {
  let src = null;
  try {
    src = readFileSync(join(heroDir, "24/outline", `${n}.svg`), "utf8");
  } catch {}
  add("Heroicons", n, src && parseSvg(src));
}
for (const n of names(TABLER)) add("Tabler", n, tabler[n]);
if (missing.length) {
  console.error("Not present in the packages:", missing.join(", "));
  process.exit(1);
}

const lit = (s) => JSON.stringify(s); // JSON strings are valid Rust string literals here
const rust = entries
  .map(({ lib, name, nodes }) => {
    const ns = nodes
      .map(([tag, attrs]) => `(${lit(tag)}, &[${attrs.map(([k, v]) => `(${lit(k)}, ${lit(v)})`).join(", ")}])`)
      .join(", ");
    return `    RawIcon { lib: Lib::${lib}, name: ${lit(name)}, nodes: &[${ns}] },`;
  })
  .join("\n");

const out = `// GENERATED by generate.mjs — do not edit by hand.
// Lucide v${version(lucideDir)} (ISC) · Heroicons v${version(heroDir)} (MIT) · Tabler Icons v${version(tablerDir)} (MIT).
// Geometry only; see the crate docs for the license notices.

use crate::{Lib, RawIcon};

pub(crate) static ICONS: &[RawIcon] = &[
${rust}
];
`;
const here = dirname(fileURLToPath(import.meta.url));
writeFileSync(join(here, "src/data.rs"), out);
console.log(`wrote ${entries.length} icons`);
