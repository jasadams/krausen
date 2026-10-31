// SPDX-License-Identifier: AGPL-3.0-or-later
// Illustrative starter entries already used by the prototype/reference recipe,
// not a complete or independently verified manufacturer specification database.
// Yield/PPG conversion is an input-boundary conversion using the same sucrose
// potential as krausen-beerxml/src/convert.rs. Brewing outputs remain Rust-only.
const SUCROSE_PPG = 46.214;
export const CUSTOM_FERMENTABLES_KEY = 'krausen.design.workspace.home.fermentables.v1';

export const STARTER_FERMENTABLES = [
  { catalogueId: 'starter-vienna', name: 'Weyermann Vienna', producer: 'Weyermann', category: 'Base malt', ppg: 36, ebc: 7 },
  { catalogueId: 'starter-carahell', name: 'Weyermann Carahell', producer: 'Weyermann', category: 'Specialty malt', ppg: 35, ebc: 25 },
  { catalogueId: 'starter-signature', name: 'Joe White Signature Lager', producer: 'Joe White', category: 'Base malt', ppg: 37, ebc: 3.6 },
  { catalogueId: 'starter-munich', name: 'Weyermann Munich I', producer: 'Weyermann', category: 'Specialty malt', ppg: 37.826, ebc: 14 },
].map(entry => Object.freeze({ ...entry, version: 1, source: 'starter', yieldPercent: entry.ppg / SUCROSE_PPG * 100 }));

export function searchFermentables(entries, query = '', filter = 'all') {
  const normalize = text => String(text).normalize('NFKD').replace(/\p{Diacritic}/gu, '').toLocaleLowerCase();
  const needle = normalize(query.trim());
  return entries.filter(entry => (filter !== 'workspace' || entry.source === 'workspace')
    && normalize([entry.name, entry.producer, entry.category].join(' ')).includes(needle));
}

export function resolveFermentable(entry, kg, rowId) {
  // Copy resolved specifications into the recipe; future library changes do not
  // retroactively change saved recipes or the immutable inputs of a brew.
  return {
    id: rowId, catalogueId: entry.catalogueId, catalogueVersion: entry.version,
    source: entry.source, name: entry.name, producer: entry.producer,
    subtitle: [entry.category, entry.producer].filter(Boolean).join(' · '),
    category: entry.category, kg,
    yieldPercent: entry.yieldPercent,
    ppg: entry.ppg ?? entry.yieldPercent / 100 * SUCROSE_PPG,
    ebc: entry.ebc,
  };
}
