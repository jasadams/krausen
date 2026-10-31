// SPDX-License-Identifier: AGPL-3.0-or-later
// Numerical reference facts from the BJCP 2021 Beer Style Guidelines.
// Verified 2026-10-31 against the official pages linked with each style.
// Only the three styles offered by this prototype are included; this is not
// a complete style catalogue. Descriptions from the guidelines are not copied.
// SRM bounds are retained as source data; EBC is converted using the existing
// calculation engine's EBC_PER_SRM = 1.97 convention (colour.rs).
const referenceStyle = (code, url, og, fg, ibu, abv, srm) => Object.freeze({
  code, edition: 'BJCP 2021', url,
  ranges: Object.freeze({
    og: { min: og[0], max: og[1], digits: 3 },
    fg: { min: fg[0], max: fg[1], digits: 3 },
    ibu: { min: ibu[0], max: ibu[1], digits: 1 },
    abv: { min: abv[0], max: abv[1], digits: 1 },
    ebc: { min: srm[0] * 1.97, max: srm[1] * 1.97, digits: 1 },
  }),
  sourceSrm: Object.freeze(srm),
});

export const STYLE_GUIDELINES = Object.freeze({
  'Munich Helles': referenceStyle('4A', 'https://www.bjcp.org/style/2021/4/4A/',
    [1.044, 1.048], [1.006, 1.012], [16, 22], [4.7, 5.4], [3, 5]),
  'German Pils': referenceStyle('5D', 'https://www.bjcp.org/style/2021/5/5D/',
    [1.044, 1.050], [1.008, 1.013], [22, 40], [4.4, 5.2], [2, 4]),
  'Märzen': referenceStyle('6A', 'https://www.bjcp.org/style/2021/6/6A/marzen/',
    [1.054, 1.060], [1.010, 1.014], [18, 24], [5.6, 6.3], [8, 17]),
});

export function guidelineFor(name) {
  return Object.hasOwn(STYLE_GUIDELINES, name) ? STYLE_GUIDELINES[name] : null;
}
