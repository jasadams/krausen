// SPDX-License-Identifier: AGPL-3.0-or-later
import init, { calculate } from './pkg/calculator.js';
import { escapeHtml as e, icon, Button, Field, Select, TextArea, Badge, Panel, Metric,
  EmptyState, Notice, Tabs, IngredientRow, Dialog, SaveStatus, StyleRange, CatalogueItem } from './components.js';
import { STYLE_GUIDELINES, guidelineFor } from './styles.js';
import { CUSTOM_FERMENTABLES_KEY, STARTER_FERMENTABLES, searchFermentables, resolveFermentable } from './catalogue.js';

const STORE = 'krausen.design.recipe.v1';
const DRAFT = 'krausen.design.draft.v1';
const HISTORY = 'krausen.design.history.v1';
const BREWS = 'krausen.design.brews.v1';
const clone = value => structuredClone(value);
const initial = {
  title: 'Munich Helles', style: 'Munich Helles', batch: 20.5, boil: 60, equipment: 'g30',
  mashWater: 15.26, mashTemp: 65, mashMinutes: 60, fermentationTemp: 12,
  grains: [
    { id: 'lager', name: 'Joe White Signature Lager', subtitle: 'Base malt · Australia', kg: 3.9, ppg: 37, ebc: 3.6 },
    { id: 'munich', name: 'Weyermann Munich I', subtitle: 'Specialty malt · Germany', kg: 0.3, ppg: 37.826, ebc: 14 },
  ],
  hops: [
    { id: 'bittering', name: 'Hallertau Mittelfrüh', grams: 25, minutes: 60, alpha: 3.8 },
    { id: 'aroma', name: 'Hallertau Mittelfrüh', grams: 25, minutes: 15, alpha: 3.8 },
  ],
  yeast: 's23', attenuation: 82,
  notes: 'A bright, easy-drinking lager with a soft malt backbone and a gentle floral finish. Keep the fermentation cool and give it a little time to settle.',
};
const equipment = {
  g30: { name: 'Grainfather G30', revision: 3, efficiency: 78, boiloff: 3, losses: 2, absorption: 0.8 },
  kettle: { name: 'Small batch kettle', revision: 1, efficiency: 72, boiloff: 2, losses: 1.2, absorption: 0.8 },
};
const yeastNames = { s23: 'Fermentis SafLager S-23', w3470: 'Fermentis SafLager W-34/70' };
const catalogue = {
  hop: [
    { name: 'Saaz', grams: 15, minutes: 10, alpha: 3.5 },
    { name: 'Tettnang', grams: 20, minutes: 15, alpha: 4.5 },
    { name: 'Hallertau Mittelfrüh', grams: 15, minutes: 10, alpha: 3.8 },
  ],
};
function read(key, fallback) { try { return JSON.parse(localStorage.getItem(key)) ?? fallback; } catch { return fallback; } }
let customFermentables = read(CUSTOM_FERMENTABLES_KEY, []);
if (!Array.isArray(customFermentables)) customFermentables = [];
let picker = { query: '', filter: 'all', selectedId: 'starter-vienna', amount: 0.25 };
function newId() { return crypto.randomUUID?.() ?? Array.from(crypto.getRandomValues(new Uint8Array(16)), byte => byte.toString(16).padStart(2, '0')).join(''); }
function allFermentables() { return [...STARTER_FERMENTABLES, ...customFermentables]; }
Object.assign(equipment, read('krausen.design.profiles.v1', {}));
let saved = read(STORE, { revision: 3, recipe: clone(initial) });
let recipe = clone(read(DRAFT, saved.recipe));
if (!recipe || !Array.isArray(recipe.grains) || !Array.isArray(recipe.hops)) recipe = clone(initial);
let history = read(HISTORY, [
  { revision: 1, title: 'First formulation', date: '12 September 2026', recipe: { ...clone(initial), boil: 90 } },
  { revision: 2, title: 'A little more Munich malt', date: '19 September 2026', recipe: { ...clone(initial), grains: initial.grains.map(g => ({ ...g, kg: g.id === 'munich' ? 0.25 : g.kg })) } },
  { revision: 3, title: 'Shorter boil, softer bitterness', date: '3 October 2026', recipe: clone(initial) },
]);
let brews = read(BREWS, []);
let route = ['recipe', 'system', 'library', 'history', 'brews'].includes(location.hash.slice(1)) ? location.hash.slice(1) : 'recipe';
let dirty = false, saving = false, failure = '', recovered = JSON.stringify(recipe) !== JSON.stringify(saved.recipe);
let derived = null, calcError = '', pending = null, addKind = 'grain', toastTimer;
let selectedSystemTab = 'default';
let runtimeReady = false;

function engineInput(model = recipe, profile = equipment[model.equipment]) {
  return {
    recipe: {
      name: model.title, batch_size_l: Number(model.batch), boil_minutes: Number(model.boil),
      mash_water_l: Number(model.mashWater),
      fermentables: model.grains.map(g => ({ name: g.name, kg: Number(g.kg), ppg: g.ppg, ebc: g.ebc })),
      hops: model.hops.map(h => ({ name: h.name, grams: Number(h.grams), alpha_acid: h.alpha / 100, minutes: Number(h.minutes), use_: 'Boil' })),
      yeast: { name: yeastNames[model.yeast], apparent_attenuation: Number(model.attenuation) / 100 },
      whirlpool_minutes: 0, whirlpool_temp_c: null,
    },
    equipment: {
      boil_off_rate_l_per_hour: profile.boiloff, grain_absorption_l_per_kg: profile.absorption,
      kettle_deadspace_l: 0, losses_l: profile.losses, mash_efficiency: profile.efficiency / 100,
      kettle_diameter_cm: 36.83, kettle_opening_cm: 36.83, boil_temp_c: 100,
      // Match the repository reference fixture's boil-only comparison. This is
      // an explicitly declared preview assumption, not a production preset.
      chilling: { Forced: { rate_per_min: 1000, plateau_c: 20 } },
    },
  };
}
function validate(model = recipe) {
  if (!String(model.title).trim()) return 'Give your recipe a name before saving.';
  if (['batch', 'boil', 'mashWater', 'attenuation', 'mashTemp', 'mashMinutes', 'fermentationTemp'].some(key => model[key] === '' || model[key] === null)) return 'Complete the numeric recipe fields to see updated estimates.';
  if (!Number.isFinite(Number(model.batch)) || Number(model.batch) <= 0) return 'Batch size must be greater than zero.';
  if (!Number.isInteger(Number(model.boil)) || Number(model.boil) < 0 || Number(model.boil) > 360) return 'Use a whole boil time between 0 and 360 minutes.';
  if (!Number.isFinite(Number(model.mashWater)) || Number(model.mashWater) < 0) return 'Mash water cannot be negative.';
  if (!Number.isFinite(Number(model.attenuation)) || model.attenuation < 0 || model.attenuation > 100) return 'Attenuation must be between 0 and 100%.';
  if (!model.grains.length || model.grains.some(g => g.kg === '' || !Number.isFinite(Number(g.kg)) || Number(g.kg) <= 0)) return 'Add fermentables with quantities greater than zero.';
  if (model.hops.some(h => h.grams === '' || h.minutes === '' || !Number.isFinite(Number(h.grams)) || h.grams < 0 || !Number.isInteger(Number(h.minutes)) || h.minutes < 0 || h.minutes > Number(model.boil))) return 'Hop quantities must be positive and additions must fall within the boil.';
  for (const key of ['mashTemp', 'mashMinutes', 'fermentationTemp']) if (!Number.isFinite(Number(model[key])) || model[key] < 0) return 'Process values must be zero or greater.';
  return '';
}
function derive(model = recipe) {
  const error = validate(model);
  if (error) throw new Error(error);
  return JSON.parse(calculate(JSON.stringify(engineInput(model))));
}
function recalculate() {
  try { derived = derive(); calcError = ''; } catch (error) { derived = null; calcError = error.message || String(error); }
}
function fmt(value, digits = 1) { return Number.isFinite(value) ? value.toFixed(digits) : '—'; }
function metricRange(key) {
  const limits = guidelineFor(recipe.style)?.ranges[key];
  return limits ? { ...limits, value: derived?.[key] ?? null } : null;
}
function styleReference() {
  const style = guidelineFor(recipe.style);
  return style ? `<span class="note-caption style-reference">Style guide: <a href="${e(style.url)}" target="_blank" rel="noopener noreferrer">${e(style.edition)} · ${e(style.code)} ${e(recipe.style)}</a></span>` : '<span class="note-caption style-reference">No style selected · create your own expression.</span>';
}
function totalGrain() { return recipe.grains.reduce((sum, g) => sum + Number(g.kg || 0), 0); }
function toast(message) {
  const node = document.querySelector('#toast'); node.textContent = message; node.hidden = false;
  clearTimeout(toastTimer); toastTimer = setTimeout(() => { node.hidden = true; }, 4200);
}
function changeState() {
  dirty = JSON.stringify(recipe) !== JSON.stringify(saved.recipe);
  try { if (dirty) localStorage.setItem(DRAFT, JSON.stringify(recipe)); else localStorage.removeItem(DRAFT); } catch { /* Recovery is best effort; explicit save reports errors. */ }
  recalculate(); updateComputed(); updateSaveState();
}
function saveStatus() { return SaveStatus({ state: saving ? 'saving' : failure ? 'error' : dirty ? 'dirty' : 'saved' }); }
function updateSaveState() {
  document.querySelectorAll('[data-save-state]').forEach(node => { node.innerHTML = saveStatus(); });
  document.querySelectorAll('[data-action="save"]').forEach(node => { node.disabled = saving || !dirty; });
}
function beerGlass() {
  return `<div class="beer-portrait" aria-hidden="true"><span class="beer-portrait__halo"></span><svg viewBox="0 0 100 140" fill="none"><path class="beer-glass__outline" d="M28 19h44l-4 91c-.3 8-8 14-18 14s-17.7-6-18-14L28 19Z"/><path class="beer-glass__liquid" d="M31 41h38l-3 67c-.3 7-7 12-16 12s-15.7-5-16-12l-3-67Z"/><path class="beer-glass__foam" d="M30 23h40l-.8 18c-5 2-8-1-12 1s-7-1-12 1-9-1-14-1L30 23Z"/><path class="beer-glass__shine" d="m37 51 2 52M40 112l4 3"/><circle class="beer-glass__bubble" cx="55" cy="66" r="1.2"/><circle class="beer-glass__bubble" cx="46" cy="89" r="1"/><circle class="beer-glass__bubble" cx="58" cy="101" r="1.2"/><path class="beer-glass__base" d="M36 130h28"/></svg></div>`;
}
function waterRows() {
  return `<div class="water-list">${[
    ['Mash water', derived?.volumes.mash_water_l], ['Sparge water', derived?.volumes.sparge_water_l],
    ['Pre-boil volume', derived?.volumes.pre_boil_l], ['Post-boil volume', derived?.volumes.post_boil_l],
  ].map(([label, value]) => `<div class="water-row"><span>${e(label)}</span><span>${fmt(value, 2)} <small>L</small></span></div>`).join('')}<div class="water-row water-row--total"><span>Total water</span><span>${fmt(derived?.volumes.total_water_l, 2)} <small>L</small></span></div></div>`;
}
function compactSummary() {
  return `${Metric({ label: 'Original gravity', value: fmt(derived?.og, 3), tone: 'compact', range: metricRange('og') })}${Metric({ label: 'Final gravity', value: fmt(derived?.fg, 3), tone: 'compact', range: metricRange('fg') })}${Metric({ label: 'Alcohol', value: fmt(derived?.abv), unit: '%', tone: 'compact', range: metricRange('abv') })}${Metric({ label: 'Bitterness', value: fmt(derived?.ibu), unit: 'IBU', tone: 'compact', range: metricRange('ibu') })}${Metric({ label: 'Colour', value: fmt(derived?.ebc), unit: 'EBC', tone: 'compact', range: metricRange('ebc') })}${Metric({ label: 'Batch', value: fmt(Number(recipe.batch)), unit: 'L', tone: 'compact' })}`;
}
function summary() {
  const profile = equipment[recipe.equipment];
  return Panel({ title: 'The beer, at a glance', eyebrow: 'YOUR FORMULATION', className: 'summary-panel',
    actions: Badge({ label: 'Live', tone: 'success', icon: 'check' }),
    body: `${beerGlass()}<div class="metric-grid">${Metric({ label: 'Original gravity', value: fmt(derived?.og, 3), id: 'metric-og', range: metricRange('og') })}${Metric({ label: 'Final gravity', value: fmt(derived?.fg, 3), id: 'metric-fg', range: metricRange('fg') })}${Metric({ label: 'Alcohol', value: fmt(derived?.abv), unit: '%', id: 'metric-abv', range: metricRange('abv') })}${Metric({ label: 'Bitterness', value: fmt(derived?.ibu), unit: 'IBU', id: 'metric-ibu', range: metricRange('ibu') })}${Metric({ label: 'Colour', value: fmt(derived?.ebc), unit: 'EBC', id: 'metric-ebc', range: metricRange('ebc') })}${Metric({ label: 'Into fermenter', value: fmt(Number(recipe.batch)), unit: 'L', id: 'metric-batch' })}</div>`,
    footer: `<div class="style-reference-stack">${styleReference()}<span class="note-caption">${icon('info', 14)} Estimates for ${e(profile.name)} · profile v${profile.revision}</span></div>` }) +
    Panel({ title: 'Water plan', actions: icon('water', 18), body: `<div id="water-results">${waterRows()}</div>`, footer: '<span class="note-caption">Includes grain absorption, boil-off and transfer losses.</span>' }) +
    Panel({ title: 'Your equipment', actions: Button({ icon: 'settings', action: 'equipment', variant: 'ghost', size: 'icon', attrs: { 'aria-label': 'Adjust equipment profile' } }),
      body: `<div class="equipment-summary"><span class="equipment-emblem">${icon('beaker', 23)}</span><div><strong>${e(profile.name)}</strong><span>Profile v${profile.revision} · current settings</span></div></div><div class="water-list equipment-values"><div class="water-row"><span>Mash efficiency</span><span>${profile.efficiency}%</span></div><div class="water-row"><span>Boil-off</span><span>${profile.boiloff} L/h</span></div><div class="water-row"><span>Transfer loss</span><span>${profile.losses} L</span></div></div>`,
      footer: '<span class="note-caption">New brews use current settings. Previous brews keep their original profile.</span>' });
}
function composition() {
  const total = totalGrain();
  return `<div class="grain-composition" role="img" aria-label="Fermentable weight proportions">${recipe.grains.map((g, i) => `<span class="grain-segment grain-segment--${i % 3}" style="flex-grow:${Math.max(0, Number(g.kg) || 0)}"></span>`).join('')}</div><div class="grain-composition__legend"><span>Grain bill</span><strong>${fmt(total, 2)} kg</strong></div>`;
}
function ingredientPanel(kind) {
  const grain = kind === 'grain';
  const rows = grain ? recipe.grains : recipe.hops;
  const content = rows.map(row => IngredientRow(grain ? {
    id: row.id, name: row.name, subtitle: `${row.subtitle} · ${row.ebc} EBC`, amount: row.kg,
    unit: 'kg', percent: totalGrain() ? fmt(Number(row.kg) / totalGrain() * 100) : '—', colour: row.ebc, kind,
  } : { id: row.id, name: row.name, subtitle: 'Pellet · Germany', amount: row.grams, unit: 'g', kind, time: row.minutes, alpha: row.alpha })).join('');
  return Panel({ title: grain ? 'Fermentables' : 'Hops', description: grain ? 'The foundation of your beer.' : 'A little bitterness. A little character.',
    actions: Button({ label: grain ? 'Add fermentable' : 'Add hop', icon: 'plus', variant: 'secondary', size: 'sm', action: `add:${kind}` }),
    body: `<div class="ingredient-table-head"><span>INGREDIENT</span><span>AMOUNT</span><span>${grain ? 'WEIGHT %' : 'BOIL TIME'}</span><span></span></div><div class="ingredient-list">${content || EmptyState({ title: 'Make room for flavour', description: 'Add an ingredient to begin.' })}</div>`,
    footer: grain ? `<div id="composition">${composition()}</div>` : `<div class="note-caption" id="hop-total">${icon('hops', 15)} ${fmt(recipe.hops.reduce((sum, hop) => sum + Number(hop.grams || 0), 0), 0)} g total · ${recipe.hops.length} additions</div>`,
  });
}
function editor() {
  return `<div class="recipe-identity"><div class="eyebrow">A RECIPE WORTH REPEATING</div><h1 class="sr-only">Edit your Munich Helles recipe</h1>${Field({ name: 'title', value: recipe.title, variant: 'title', attrs: { 'aria-label': 'Recipe name', maxlength: 100 } })}<div class="recipe-subtitle">${Badge({ label: 'All grain', tone: 'accent', icon: 'grain' })}<span>Clean, golden, quietly brilliant.</span></div></div>
  <div class="recipe-settings">${Field({ label: 'Batch size', name: 'batch', value: recipe.batch, type: 'number', unit: 'L', min: 1, step: 0.5, hint: 'Into the fermenter' })}${Field({ label: 'Boil time', name: 'boil', value: recipe.boil, type: 'number', unit: 'min', min: 0, step: 1 })}${Select({ label: 'Equipment profile', name: 'equipment', value: recipe.equipment, options: Object.entries(equipment).map(([value, p]) => ({ value, label: p.name })) })}${Select({ label: 'Beer style', name: 'style', value: recipe.style, options: [{ value: '', label: 'No style / experimental' }, ...Object.keys(STYLE_GUIDELINES).map(name => ({ value: name, label: name }))] })}</div>
  ${Tabs({ items: [{ id: 'recipe', label: 'Recipe' }, { id: 'history', label: 'History', count: history.length }, { id: 'brews', label: 'Brews', count: brews.length }], active: 'recipe' })}
  <div class="mobile-summary" id="compact-summary" aria-label="Calculated recipe targets">${compactSummary()}</div><div class="mobile-only" id="mobile-style-reference">${styleReference()}</div>
  <div class="mobile-only mobile-save-state" data-save-state>${saveStatus()}</div>
  <div id="editor-notice">${recovered ? Notice({ title: 'Your unsaved draft is back', body: 'Keep working, or save it when you are ready.', tone: 'info' }) : ''}</div>
  <div id="calculation-error">${calcError ? Notice({ title: 'A little more information needed', body: calcError, tone: 'warning' }) : ''}</div>
  <div class="recipe-layout"><div class="recipe-main">${ingredientPanel('grain')}${ingredientPanel('hop')}
    ${Panel({ title: 'Yeast', description: 'The finishing touch.', body: `<div class="yeast-layout"><span class="yeast-emblem">${icon('flask', 26)}</span>${Select({ label: 'Yeast strain', name: 'yeast', value: recipe.yeast, options: Object.entries(yeastNames).map(([value, label]) => ({ value, label })) })}${Field({ label: 'Attenuation', name: 'attenuation', value: recipe.attenuation, type: 'number', unit: '%', min: 0, step: 1, attrs: { max: 100 } })}</div>` })}
    ${Panel({ title: 'Process', description: 'A few details to make it yours.', body: `<div class="form-grid four-col">${Field({ label: 'Mash temperature', name: 'mashTemp', value: recipe.mashTemp, type: 'number', unit: '°C', step: 0.5, min: 0 })}${Field({ label: 'Mash time', name: 'mashMinutes', value: recipe.mashMinutes, type: 'number', unit: 'min', step: 1, min: 0 })}${Field({ label: 'Mash water', name: 'mashWater', value: recipe.mashWater, type: 'number', unit: 'L', step: 0.1, min: 0 })}${Field({ label: 'Fermentation', name: 'fermentationTemp', value: recipe.fermentationTemp, type: 'number', unit: '°C', step: 0.5, min: 0 })}</div>` })}
    ${Panel({ title: 'Recipe notes', body: TextArea({ name: 'notes', label: 'What makes this one special?', value: recipe.notes, rows: 3, placeholder: 'Your intent, process notes, and ideas for next time…' }) })}
    <footer class="page-footer"><span>${icon('history', 15)} Saved revision ${saved.revision} · edited by you</span>${Button({ label: 'View history', icon: 'arrow-right', variant: 'ghost', size: 'sm', action: 'navigate:history' })}</footer>
  </div><aside class="recipe-aside" aria-label="Calculated recipe targets"><div id="summary">${summary()}</div>${Button({ label: 'Start a brew', icon: 'flask', variant: 'secondary', action: 'start-brew', className: 'full-width' })}<p class="aside-caption">A new brew keeps a snapshot of this recipe and your equipment.</p></aside></div>`;
}
function system() {
  const swatches = [['Background', 'background'], ['Surface', 'surface'], ['Ink', 'foreground'], ['Copper', 'primary'], ['Sage', 'olive'], ['Border', 'border']];
  const metricData = derived || derive(initial);
  return `<header class="page-heading"><span class="eyebrow">KRAUSEN / DESIGN SYSTEM / 01</span><h1 class="page-title">Good design, <em>on repeat.</em></h1><p class="page-description">One considered system. Every ingredient, every interaction, every screen.</p><div class="demo-row">${Badge({ label: 'Living component library', tone: 'accent', icon: 'palette' })}${Badge({ label: 'Approved direction', tone: 'neutral' })}</div></header>
  <div class="system-principles"><div><span class="token-label">01 / WARM & PRECISE</span><p>A little craft. A lot of clarity.</p></div><div><span class="token-label">02 / COMPONENT FIRST</span><p>Same building blocks. Everywhere.</p></div><div><span class="token-label">03 / QUIETLY USEFUL</span><p>Nothing between you and the recipe.</p></div></div>
  <div class="showcase-grid">
  ${Panel({ title: 'A grounded palette', eyebrow: 'FOUNDATIONS / COLOUR', description: 'Semantic tokens, rather than page-specific colours.', body: `<div class="swatch-grid">${swatches.map(([label, token]) => `<div class="swatch"><span class="swatch__colour swatch__colour--${token}"></span><strong>${label}</strong><code>--color-${token}</code></div>`).join('')}</div><div class="demo-row tone-demo">${Badge({ label: 'Saved', tone: 'success', icon: 'check' })}${Badge({ label: 'Unsaved', tone: 'warning' })}${Badge({ label: 'All grain', tone: 'accent', icon: 'grain' })}</div>` })}
  ${Panel({ title: 'Type with a point of view', eyebrow: 'FOUNDATIONS / TYPOGRAPHY', body: '<div class="type-specimen"><span class="display-specimen">Made to be brewed.</span><span class="type-specimen__body">DM Sans keeps the everyday clear and effortless.</span><span class="number-specimen">1.045 &nbsp; 20.5 L &nbsp; 78%</span></div>', footer: '<span class="token-label">INSTRUMENT SERIF / DM SANS / GEIST MONO</span>' })}
  ${Panel({ title: 'Actions with hierarchy', eyebrow: 'COMPONENT / BUTTON', description: 'A deliberate primary action. Supporting controls stay quiet.', body: `<div class="component-demo"><div class="demo-row">${Button({ label: 'Save recipe', icon: 'check', action: 'demo:Recipe saved', variant: 'primary' })}${Button({ label: 'Add ingredient', icon: 'plus', action: 'demo:Add an ingredient', variant: 'secondary' })}${Button({ label: 'View history', icon: 'history', action: 'demo:History opened', variant: 'ghost' })}</div><div class="demo-row">${Button({ label: 'Saving…', disabled: true })}${Button({ icon: 'settings', variant: 'secondary', size: 'icon', action: 'equipment', attrs: { 'aria-label': 'Open equipment settings' } })}${Button({ label: 'Delete', variant: 'destructive', size: 'sm', action: 'demo:Destructive actions use a deliberate confirmation' })}</div><code class="token-label">Button · variant / size / icon / disabled</code></div>` })}
  ${Panel({ title: 'Forms without the friction', eyebrow: 'COMPONENT / FIELD & SELECT', body: `<div class="form-grid two-col">${Field({ label: 'Batch size', name: 'demo-batch', value: 20.5, type: 'number', unit: 'L', hint: 'Into the fermenter' })}${Select({ label: 'Equipment', name: 'demo-equipment', value: 'g30', options: [{ value: 'g30', label: 'Grainfather G30' }, { value: 'kettle', label: 'Small batch kettle' }] })}${Field({ label: 'Hop amount', name: 'demo-error', value: -5, type: 'number', unit: 'g', error: 'Use zero or more.' })}${Field({ label: 'Original gravity', name: 'demo-readonly', value: '1.045', attrs: { readonly: true }, hint: 'Calculated from your recipe' })}</div>` })}
  ${Panel({ title: 'An ingredient, not a form', eyebrow: 'COMPONENT / INGREDIENT ROW', description: 'Identity, specification, quantity and contribution in one reusable row.', body: IngredientRow({ id: 'demo-pilsner', name: 'Weyermann Pilsner', subtitle: 'Base malt · Germany · 3.6 EBC', amount: 3.9, percent: '92.9', colour: 3.6, kind: 'grain' }) + IngredientRow({ id: 'demo-hop', name: 'Hallertau Mittelfrüh', subtitle: 'Pellet · Germany', amount: 25, unit: 'g', kind: 'hop', time: 60, alpha: 3.8 }), footer: '<code class="token-label">IngredientRow · grain / hop · editable / historical</code>' })}
  ${Panel({ title: 'Numbers that earn their space', eyebrow: 'COMPONENT / METRIC', body: `<div class="metric-grid three-col">${Metric({ label: 'Original gravity', value: fmt(metricData.og, 3) })}${Metric({ label: 'Alcohol', value: fmt(metricData.abv), unit: '%' })}${Metric({ label: 'Bitterness', value: fmt(metricData.ibu), unit: 'IBU' })}</div><p class="note-caption">The same metric component used beside your recipe.</p>`, footer: '<code class="token-label">Metric · label / value / unit / hint</code>' })}
  ${Panel({ title: 'A library that grows with you', eyebrow: 'COMPONENT / CATALOGUE ITEM', description: 'Search the starter catalogue, or save the ingredients you use in your own library.', body: `<div class="k-catalogue-list">${CatalogueItem({ id: 'demo-starter', name: 'Weyermann Vienna', producer: 'Weyermann', category: 'Base malt', colour: 7, yieldPercent: 77.9, source: 'starter', action: 'demo:Choose a catalogue ingredient from Add fermentable.' })}${CatalogueItem({ id: 'demo-own', name: 'My local pale malt', producer: 'Your maltster', category: 'Base malt', colour: 4.2, yieldPercent: 81, source: 'workspace', selected: true, action: 'demo:Custom ingredients stay in your library for the next recipe.' })}</div>`, footer: '<code class="token-label">CatalogueItem · starter / workspace · selected / unselected</code>' })}
  ${Panel({ title: 'Find your place in a style', eyebrow: 'COMPONENT / STYLE RANGE', description: 'A reference band, a moving marker, and a clear status. Guidance for the beer you want to make.', body: `<div class="metric-grid two-col">${Metric({ label: 'Original gravity', value: '1.046', range: { min: 1.044, max: 1.048, value: 1.046, digits: 3 } })}${Metric({ label: 'Bitterness', value: '15.0', unit: 'IBU', range: { min: 16, max: 22, value: 15, digits: 1 } })}${Metric({ label: 'Alcohol', value: '6.0', unit: '%', range: { min: 4.7, max: 5.4, value: 6, digits: 1 } })}${Metric({ label: 'Awaiting estimate', value: '—', range: { min: 1.044, max: 1.048, value: null, digits: 3 } })}</div>`, footer: '<code class="token-label">StyleRange · within / below / above / awaiting · reference limits + current marker</code>' })}
  ${Panel({ title: 'Every state, considered', eyebrow: 'COMPONENT / STATUS & NOTICE', body: `<div class="component-demo">${SaveStatus({ state: 'saved' })}${SaveStatus({ state: 'dirty' })}${SaveStatus({ state: 'saving' })}${SaveStatus({ state: 'conflict' })}${Notice({ title: 'Your draft is safe', body: 'A newer revision is available. Your edits have been kept.', tone: 'warning' })}</div>` })}
  ${Panel({ title: 'A place for what comes next', eyebrow: 'COMPONENT / EMPTY STATE', body: EmptyState({ title: 'Your first brew starts here', description: 'Save a recipe, then make it real. Its original targets will stay with the brew.', action: Button({ label: 'Open recipe', icon: 'arrow-right', variant: 'secondary', action: 'navigate:recipe' }) }) })}
  ${Panel({ title: 'Navigation, consistently', eyebrow: 'COMPONENT / TABS', body: Tabs({ items: [{ id: 'demo-default', label: 'Recipe' }, { id: 'demo-history', label: 'History', count: 3 }, { id: 'demo-brews', label: 'Brews', count: 0 }], active: `demo-${selectedSystemTab}` }) + '<p class="note-caption">Keyboard accessible, with a visible selected state.</p>' })}
  ${Panel({ title: 'A pause before leaving', eyebrow: 'COMPONENT / DIALOG', description: 'Native focus management, a clear choice, and no lost work.', body: `<div class="demo-row">${Button({ label: 'Preview unsaved warning', icon: 'eye', variant: 'secondary', action: 'demo-dialog' })}${Button({ label: 'Try dark mode', icon: 'moon', variant: 'ghost', action: 'theme' })}</div>` })}
  </div><footer class="page-footer"><span>These are the actual components used by the recipe page.</span>${Button({ label: 'Back to the recipe', icon: 'arrow-right', variant: 'ghost', action: 'navigate:recipe' })}</footer>`;
}
function library() {
  let result; try { result = derive(saved.recipe); } catch { result = null; }
  return `<header class="page-heading"><span class="eyebrow">YOUR BREWING NOTEBOOK</span><h1 class="page-title">Good things <em>start here.</em></h1><p class="page-description">Recipes to refine, return to, and make your own.</p></header><div class="library-toolbar">${Field({ name: 'recipe-search', value: '', type: 'search', placeholder: 'Find a recipe…', attrs: { 'aria-label': 'Find a recipe' } })}${Button({ label: 'Open recipe', icon: 'arrow-right', action: 'navigate:recipe' })}</div><div id="library-results" class="library-grid">${Panel({ title: saved.recipe.title, eyebrow: `${saved.recipe.style.toUpperCase()} / ALL GRAIN`, body: `<div class="recipe-card-preview">${beerGlass()}<div class="metric-grid three-col">${Metric({ label: 'Batch', value: fmt(Number(saved.recipe.batch)), unit: 'L' })}${Metric({ label: 'Alcohol', value: fmt(result?.abv), unit: '%' })}${Metric({ label: 'Bitterness', value: fmt(result?.ibu), unit: 'IBU' })}</div></div>`, footer: `<span class="note-caption">Revision ${saved.revision} · ${brews.length} brews</span>${Button({ label: 'Open recipe', icon: 'arrow-right', variant: 'ghost', action: 'navigate:recipe' })}`, className: 'recipe-card' })}</div><p class="note-caption">This design preview contains one editable recipe. Saves stay in this browser.</p>`;
}
function historyPage() {
  return `<header class="page-heading"><span class="eyebrow">${e(saved.recipe.title.toUpperCase())} / HISTORY</span><h1 class="page-title">A recipe <em>in the making.</em></h1><p class="page-description">Every saved formulation. Every brew that made it real.</p></header>${Tabs({ items: [{ id: 'recipe', label: 'Recipe' }, { id: 'history', label: 'History', count: history.length }, { id: 'brews', label: 'Brews', count: brews.length }], active: 'history' })}${Panel({ title: 'Recipe history', body: `<div class="history-list">${[...history].reverse().map(item => `<div class="history-entry"><span class="history-dot">${icon('history', 16)}</span><div class="history-content"><div class="demo-row"><strong>Revision ${item.revision}</strong>${item.revision === saved.revision ? Badge({ label: 'Current', tone: 'success' }) : ''}${brews.filter(b => b.revision === item.revision).map(b => Badge({ label: `Brew ${b.number}`, icon: 'flask', tone: 'accent' })).join('')}</div><h3>${e(item.title)}</h3><p>${e(item.date)} · You</p></div>${Button({ label: 'Inspect', variant: 'ghost', size: 'sm', action: `inspect:${item.revision}` })}${item.revision !== saved.revision ? Button({ label: 'Restore', variant: 'secondary', size: 'sm', action: `restore:${item.revision}` }) : ''}</div>`).join('')}</div>`, footer: '<span class="note-caption">Restoring creates a new revision. Original revisions and brew markers stay unchanged.</span>' })}`;
}
function brewsPage() {
  return `<header class="page-heading"><span class="eyebrow">${e(saved.recipe.title.toUpperCase())} / BREWS</span><h1 class="page-title">From recipe <em>to reality.</em></h1><p class="page-description">The original plan, what you measured, and a little room for notes.</p></header>${Tabs({ items: [{ id: 'recipe', label: 'Recipe' }, { id: 'history', label: 'History', count: history.length }, { id: 'brews', label: 'Brews', count: brews.length }], active: 'brews' })}${brews.length ? brews.map(brew => Panel({ title: `Brew ${brew.number} · ${brew.recipe.title}`, eyebrow: `${brew.date} / RECIPE REVISION ${brew.revision}`, actions: Badge({ label: 'In progress', tone: 'accent' }), body: `<div class="brew-metrics">${Metric({ label: 'OG target', value: fmt(brew.derived.og, 3) })}${Metric({ label: 'FG target', value: fmt(brew.derived.fg, 3) })}${Metric({ label: 'Batch target', value: fmt(Number(brew.recipe.batch)), unit: 'L' })}</div><p class="note-caption">${e(brew.equipment.name)} · profile v${brew.equipment.revision} · original targets frozen</p>`, footer: Button({ label: 'Record actuals', icon: 'plus', variant: 'secondary', size: 'sm', action: `actuals:${brew.number}` }) })).join('') : Panel({ body: EmptyState({ title: 'Ready when you are', description: 'Your first brew will pin this recipe and its equipment settings in history.', action: Button({ label: 'Start a brew', icon: 'flask', action: 'start-brew' }) }) })}`;
}
function shell(body) {
  const isRecipe = route === 'recipe';
  const nav = (label, name, glyph) => Button({ label, icon: glyph, variant: 'ghost', action: `navigate:${name}`, className: `nav-link${route === name || name === 'library' && ['recipe', 'history', 'brews'].includes(route) ? ' is-active' : ''}`, attrs: route === name ? { 'aria-current': 'page' } : {} });
  return `<div class="app-shell"><aside class="sidebar" aria-label="Main navigation"><a href="#recipe" class="brand" data-action="navigate:recipe"><img class="brand-mark" src="assets/mark.svg" alt=""><span>krausen<span class="brand-dot">.</span></span></a><div class="workspace-switch"><span class="workspace-avatar">B</span><div><strong>The home brewery</strong><span>Personal workspace</span></div>${icon('chevron-down', 14)}</div><nav class="nav-group"><span class="nav-label">YOUR BREWERY</span>${nav('Recipes', 'library', 'recipe')}${nav('Brews', 'brews', 'flask')}${Button({ label: 'Equipment', icon: 'beaker', variant: 'ghost', action: 'equipment', className: 'nav-link' })}</nav><div class="sidebar-note"><span class="small-grain">${icon('grain', 32)}</span><p>A little curiosity.<br>A better next batch.</p></div><nav class="nav-group sidebar-bottom"><span class="nav-label">THE DETAILS</span>${nav('Design system', 'system', 'palette')}${Button({ label: document.documentElement.dataset.theme === 'dark' ? 'Light appearance' : 'Dark appearance', icon: document.documentElement.dataset.theme === 'dark' ? 'sun' : 'moon', variant: 'ghost', action: 'theme', className: 'nav-link' })}<div class="sidebar-user"><span class="workspace-avatar">B</span><div><strong>Sample brewer</strong><span>Home brewer</span></div>${Badge({ label: 'Preview', tone: 'neutral' })}</div></nav></aside><div class="main"><header class="topbar"><div class="breadcrumb">${Button({ icon: 'menu', variant: 'ghost', size: 'icon', action: 'mobile-nav', className: 'mobile-only', attrs: { 'aria-label': 'Open navigation' } })}${Button({ label: route === 'system' ? 'Design system' : 'Recipes', variant: 'ghost', size: 'sm', action: route === 'system' ? 'navigate:system' : 'navigate:library' })}${route !== 'system' && route !== 'library' ? `<span class="breadcrumb-separator">/</span><span class="breadcrumb-current">${e(recipe.title)}</span>` : ''}</div><div class="topbar-actions">${isRecipe ? `<span data-save-state>${saveStatus()}</span>${Button({ icon: 'history', variant: 'ghost', size: 'icon', action: 'navigate:history', attrs: { 'aria-label': 'Recipe history' } })}${Button({ label: 'Save recipe', icon: 'check', action: 'save', disabled: !dirty || saving })}` : Button({ label: 'Open recipe', icon: 'arrow-up-right', variant: 'secondary', size: 'sm', action: 'navigate:recipe' })}</div></header><main id="main" class="page${route === 'system' ? ' page--system' : ''}" tabindex="-1">${body}</main><footer class="prototype-footer"><span>KRAUSEN · DESIGN PREVIEW</span><span>Thoughtful recipes. Better brews.</span></footer></div></div>`;
}
function render() {
  recalculate(); dirty = JSON.stringify(recipe) !== JSON.stringify(saved.recipe);
  document.title = `${route === 'system' ? 'Design system' : recipe.title} · Krausen`;
  const views = { recipe: editor, system, library, history: historyPage, brews: brewsPage };
  document.querySelector('#app').innerHTML = shell((views[route] || editor)());
  updateSaveState();
}
function updateComputed() {
  const summaryNode = document.querySelector('#summary'); if (summaryNode) summaryNode.innerHTML = summary();
  const errorNode = document.querySelector('#calculation-error'); if (errorNode) errorNode.innerHTML = calcError ? Notice({ title: 'A little more information needed', body: calcError, tone: 'warning' }) : '';
  const compositionNode = document.querySelector('#composition'); if (compositionNode) compositionNode.innerHTML = composition();
  const compact = document.querySelector('#compact-summary'); if (compact) compact.innerHTML = compactSummary();
  const styleCaption = document.querySelector('#mobile-style-reference'); if (styleCaption) styleCaption.innerHTML = styleReference();
  const hopTotal = document.querySelector('#hop-total'); if (hopTotal) hopTotal.innerHTML = `${icon('hops', 15)} ${fmt(recipe.hops.reduce((sum, hop) => sum + Number(hop.grams || 0), 0), 0)} g total · ${recipe.hops.length} additions`;
  for (const grain of recipe.grains) {
    const row = document.querySelector(`[data-ingredient-id="${CSS.escape(grain.id)}"] .k-ingredient-row__percent`);
    if (row) row.textContent = totalGrain() ? `${fmt(Number(grain.kg) / totalGrain() * 100)}%` : '—';
  }
  document.querySelectorAll('.recipe-settings input').forEach(input => { input.setAttribute('aria-invalid', String(!input.validity.valid)); });
  const announcer = document.querySelector('#calculation-announcement');
  announcer.textContent = derived ? `Updated estimates. Original gravity ${fmt(derived.og, 3)}, alcohol ${fmt(derived.abv)} percent.` : calcError;
}
function closeDialog() { const dialog = document.querySelector('dialog[open]'); if (dialog) dialog.close(); }
function showDialog(markup) {
  closeDialog(); document.querySelector('#dialogs').innerHTML = markup;
  const dialog = document.querySelector('#dialogs dialog');
  dialog.addEventListener('click', event => { if (event.target === dialog) { const r = dialog.getBoundingClientRect(); if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) dialog.close(); } });
  dialog.showModal();
}
function unsavedDialog(target, demo = false) {
  pending = target;
  showDialog(Dialog({ id: 'unsaved-dialog', title: 'Keep your changes?', description: 'You have unsaved changes to this recipe. Save them before you leave, or discard this draft.',
    body: Notice({ title: recipe.title, body: 'Your saved recipe stays unchanged until you choose Save.', tone: 'info' }),
    actions: Button({ label: 'Stay here', variant: 'ghost', action: 'close-dialog' }) + Button({ label: 'Discard changes', variant: 'secondary', action: demo ? 'demo-dismiss' : 'discard-leave' }) + Button({ label: 'Save & continue', icon: 'check', action: demo ? 'demo-dismiss' : 'save-leave' }) }));
}
function commitNavigation(target) {
  closeDialog(); route = target;
  historyPush(target); render(); window.scrollTo({ top: 0 }); document.querySelector('#main')?.focus({ preventScroll: true });
}
function historyPush(target) { window.history.pushState({ route: target }, '', `#${target}`); }
function requestNavigation(target) {
  if (target === route) return;
  if (dirty) unsavedDialog({ type: 'navigate', target }); else commitNavigation(target);
}
async function save() {
  if (saving) return false;
  if (calcError || validate()) { failure = calcError || validate(); updateSaveState(); toast(failure); return false; }
  if (!dirty) return true;
  const existing = read(STORE, null);
  if (existing && existing.revision !== saved.revision) { failure = 'A newer revision is saved in another tab. Your draft has been kept.'; updateSaveState(); toast(failure); return false; }
  saving = true; failure = ''; updateSaveState();
  await new Promise(resolve => setTimeout(resolve, 220));
  const next = { revision: saved.revision + 1, recipe: clone(recipe) };
  const nextHistory = [...history, { revision: next.revision, title: 'Recipe refined', date: new Date().toLocaleDateString('en-AU', { day: 'numeric', month: 'long', year: 'numeric' }), recipe: clone(recipe) }];
  try {
    // Local preview persistence only. Production uses an atomic shared service.
    localStorage.setItem(STORE, JSON.stringify(next)); localStorage.setItem(HISTORY, JSON.stringify(nextHistory)); localStorage.removeItem(DRAFT);
    saved = next; history = nextHistory; dirty = false; recovered = false; saving = false; failure = ''; updateSaveState();
    toast(`Revision ${saved.revision} saved in this browser.`); return true;
  } catch { saving = false; failure = 'This browser could not save the recipe. Your draft is still open.'; updateSaveState(); toast(failure); return false; }
}
function openAdd(kind) {
  addKind = kind;
  if (kind === 'grain') {
    const latest = read(CUSTOM_FERMENTABLES_KEY, customFermentables);
    if (Array.isArray(latest)) customFermentables = latest;
    picker = { query: '', filter: 'all', selectedId: 'starter-vienna', amount: 0.25 };
    return openFermentablePicker();
  }
  showDialog(Dialog({ id: 'ingredient-dialog', title: kind === 'grain' ? 'Build your grain bill' : 'A little more character', description: 'Choose an ingredient, then make the quantity your own.', body: Select({ label: kind === 'grain' ? 'Fermentable' : 'Hop', name: 'catalogue-choice', value: '0', options: catalogue[kind].map((item, index) => ({ value: String(index), label: item.name })) }), actions: Button({ label: 'Cancel', variant: 'ghost', action: 'close-dialog' }) + Button({ label: 'Add ingredient', icon: 'plus', action: 'confirm-add' }) }));
}
function catalogueResults() {
  const entries = searchFermentables(allFermentables(), picker.query, picker.filter);
  return entries.length ? entries.map(entry => CatalogueItem({
    id: entry.catalogueId, name: entry.name, producer: entry.producer,
    category: entry.category, colour: entry.ebc, yieldPercent: entry.yieldPercent,
    source: entry.source, selected: entry.catalogueId === picker.selectedId,
  })).join('') : EmptyState({ title: 'Make this library your own', description: 'No matching fermentables. Add the malt or grain you use.', action: Button({ label: 'Create fermentable', icon: 'plus', action: 'create-fermentable', variant: 'secondary' }) });
}
function updateCatalogueResults() {
  const entries = searchFermentables(allFermentables(), picker.query, picker.filter);
  if (!entries.some(entry => entry.catalogueId === picker.selectedId)) picker.selectedId = null;
  const results = document.querySelector('#catalogue-results'); if (results) results.innerHTML = catalogueResults();
  const count = document.querySelector('#catalogue-result-count'); if (count) count.textContent = `${entries.length} fermentable${entries.length === 1 ? '' : 's'}`;
  const selected = allFermentables().find(entry => entry.catalogueId === picker.selectedId);
  const label = document.querySelector('#catalogue-selected'); if (label) label.textContent = selected ? selected.name : 'Choose a fermentable';
  const add = document.querySelector('[data-action="add-selected-fermentable"]'); if (add) add.disabled = !selected;
}
function openFermentablePicker() {
  const selected = allFermentables().find(entry => entry.catalogueId === picker.selectedId);
  const count = searchFermentables(allFermentables(), picker.query, picker.filter).length;
  showDialog(Dialog({ id: 'ingredient-dialog', title: 'Build your grain bill', wide: true,
    description: 'Find a fermentable in the starter catalogue, or add one to your own library.',
    body: `<div class="k-catalogue-toolbar">${Field({ label: 'Search fermentables', name: 'library-search', type: 'search', value: picker.query, placeholder: 'Search by name, producer or type…' })}${Button({ label: 'Create fermentable', icon: 'plus', action: 'create-fermentable', variant: 'secondary' })}</div><div class="k-catalogue-filters">${Button({ label: 'All fermentables', action: 'catalogue-filter:all', size: 'sm', variant: picker.filter === 'all' ? 'secondary' : 'ghost', attrs: { 'aria-pressed': picker.filter === 'all' } })}${Button({ label: 'Your library', action: 'catalogue-filter:workspace', size: 'sm', variant: picker.filter === 'workspace' ? 'secondary' : 'ghost', attrs: { 'aria-pressed': picker.filter === 'workspace' } })}<span class="note-caption" id="catalogue-result-count" role="status">${count} fermentable${count === 1 ? '' : 's'}</span></div><div id="catalogue-results" class="k-catalogue-list">${catalogueResults()}</div><div id="catalogue-error"></div><div class="k-catalogue-selection"><div><span class="note-caption">SELECTED FERMENTABLE</span><strong id="catalogue-selected">${e(selected?.name ?? 'Choose a fermentable')}</strong></div>${Field({ label: 'Amount in recipe', name: 'library-amount', type: 'number', value: picker.amount, unit: 'kg', step: 0.01, min: 0.001 })}</div><p class="note-caption">Specifications are copied into your recipe, so later library changes cannot change what you brewed.</p>`,
    actions: Button({ label: 'Cancel', action: 'close-dialog', variant: 'ghost' }) + Button({ label: 'Add ingredient', icon: 'plus', action: 'add-selected-fermentable', disabled: !selected }),
  }));
}
function createFermentableDialog() {
  showDialog(Dialog({ id: 'custom-fermentable-dialog', title: 'A fermentable of your own', wide: true,
    description: 'Save a malt or grain to your library, then use it in any recipe.',
    body: `<div id="library-error"></div><div class="form-grid two-col">${Field({ label: 'Fermentable name', name: 'library-name', value: picker.query, placeholder: 'e.g. Local maltster pale malt', attrs: { maxlength: 100 } })}${Field({ label: 'Producer', name: 'library-producer', placeholder: 'Maltster or supplier (optional)', attrs: { maxlength: 100 } })}${Select({ label: 'Type', name: 'library-category', value: 'Base malt', options: ['Base malt', 'Specialty malt', 'Flaked grain'] })}${Field({ label: 'Colour', name: 'library-colour', type: 'number', value: '', placeholder: '3.5', unit: 'EBC', min: 0, step: 0.1 })}${Field({ label: 'Potential yield', name: 'library-yield', type: 'number', value: '', placeholder: '80', unit: '%', min: 0.01, step: 0.1, attrs: { max: 100 }, hint: 'Use the extract yield from the supplier’s specification.' })}</div><p class="note-caption">${icon('folder', 14)} Stored in your workspace’s ingredient library. It will not modify the starter catalogue.</p>`,
    actions: Button({ label: 'Back to catalogue', icon: 'left', action: 'back-to-catalogue', variant: 'ghost' }) + Button({ label: 'Save to library', icon: 'check', action: 'save-fermentable' }),
  }));
}
function saveCustomFermentable() {
  const field = name => document.querySelector(`[name="library-${name}"]`).value;
  const name = field('name').trim(), producer = field('producer').trim(), category = field('category');
  const colourText = field('colour'), yieldText = field('yield');
  const ebc = Number(colourText), yieldPercent = Number(yieldText);
  const latest = read(CUSTOM_FERMENTABLES_KEY, customFermentables);
  if (Array.isArray(latest)) customFermentables = latest;
  let error = !name ? 'Give your fermentable a name.' : '';
  if (!error && (colourText === '' || !Number.isFinite(ebc) || ebc < 0)) error = 'Enter a colour of zero or more EBC.';
  if (!error && (yieldText === '' || !Number.isFinite(yieldPercent) || yieldPercent <= 0 || yieldPercent > 100)) error = 'Enter a potential yield greater than zero and at most 100%.';
  if (!error && customFermentables.some(entry => entry.name.toLocaleLowerCase() === name.toLocaleLowerCase() && entry.producer.toLocaleLowerCase() === producer.toLocaleLowerCase())) error = 'That fermentable and producer are already in your library.';
  if (error) { document.querySelector('#library-error').innerHTML = Notice({ title: 'A little more information needed', body: error, tone: 'warning' }); return; }
  const entry = { catalogueId: `custom-${newId()}`, version: 1, source: 'workspace', name, producer, category, ebc, yieldPercent, createdAt: new Date().toISOString() };
  const next = [...customFermentables, entry];
  try { localStorage.setItem(CUSTOM_FERMENTABLES_KEY, JSON.stringify(next)); }
  catch { document.querySelector('#library-error').innerHTML = Notice({ title: 'Could not save the fermentable', body: 'Your entered specifications are still here. Please try again.', tone: 'warning' }); return; }
  customFermentables = next;
  picker = { ...picker, query: '', filter: 'workspace', selectedId: entry.catalogueId };
  openFermentablePicker(); toast(`${name} saved to your library.`);
}
function equipmentDialog() {
  const p = equipment[recipe.equipment];
  showDialog(Dialog({ id: 'equipment-dialog', title: p.name, description: `Profile v${p.revision}. Changes update this recipe's estimates. Existing brews keep their original settings.`, body: `<div class="form-grid two-col">${Field({ label: 'Mash efficiency', name: 'profile-efficiency', value: p.efficiency, type: 'number', min: 1, unit: '%', attrs: { max: 100 } })}${Field({ label: 'Boil-off rate', name: 'profile-boiloff', value: p.boiloff, type: 'number', min: 0, step: 0.1, unit: 'L/h' })}${Field({ label: 'Transfer loss', name: 'profile-losses', value: p.losses, type: 'number', min: 0, step: 0.1, unit: 'L' })}${Field({ label: 'Grain absorption', name: 'profile-absorption', value: p.absorption, type: 'number', min: 0, step: 0.1, unit: 'L/kg' })}</div>`, actions: Button({ label: 'Cancel', variant: 'ghost', action: 'close-dialog' }) + Button({ label: 'Update profile', icon: 'check', action: 'profile-save' }) }));
}
function startBrew() {
  if (dirty) { unsavedDialog({ type: 'brew' }); return; }
  if (!derived) { toast('Complete the recipe before starting a brew.'); return; }
  const snapshot = { number: brews.length + 1, revision: saved.revision, date: new Date().toLocaleDateString('en-AU'), recipe: clone(saved.recipe), equipment: clone(equipment[recipe.equipment]), derived: clone(derived) };
  brews = [...brews, snapshot]; try { localStorage.setItem(BREWS, JSON.stringify(brews)); } catch { toast('Brew preview could not be saved.'); return; }
  commitNavigation('brews'); toast('Brew started. Its recipe, equipment and targets are frozen.');
}
async function continuePending(discard = false) {
  if (!discard && !(await save())) { closeDialog(); return; }
  if (discard) { recipe = clone(saved.recipe); dirty = false; failure = ''; recovered = false; try { localStorage.removeItem(DRAFT); } catch {} recalculate(); }
  const action = pending; pending = null; closeDialog();
  if (action?.type === 'brew') startBrew(); else if (action?.type === 'navigate') commitNavigation(action.target);
}
function inspectRevision(number) {
  const item = history.find(h => h.revision === number); if (!item) return;
  showDialog(Dialog({ id: 'revision-dialog', title: `Revision ${number}`, description: `${item.title} · ${item.date}`, body: `<div class="history-ingredients">${item.recipe.grains.map(g => IngredientRow({ id: `past-${g.id}`, name: g.name, subtitle: g.subtitle, amount: g.kg, colour: g.ebc, kind: 'grain', editable: false })).join('')}</div><p class="note-caption">Saved formulation · ${item.recipe.batch} L into fermenter · ${item.recipe.boil} minute boil. Brew snapshots retain their original calculation context.</p>`, actions: Button({ label: 'Close', variant: 'secondary', action: 'close-dialog' }) }));
}
function actualsDialog(number) {
  const brew = brews.find(b => b.number === number); if (!brew) return;
  showDialog(Dialog({ id: 'actuals-dialog', title: `Brew ${number} · actuals & notes`, description: 'Record what you measured. Leave anything else blank.', body: `<div class="form-grid three-col">${Field({ label: `Original gravity · target ${fmt(brew.derived.og, 3)}`, name: 'actual-og', value: brew.actuals?.og ?? '', type: 'number', step: 0.001, placeholder: '1.045' })}${Field({ label: `Final gravity · target ${fmt(brew.derived.fg, 3)}`, name: 'actual-fg', value: brew.actuals?.fg ?? '', type: 'number', step: 0.001, placeholder: 'Not measured' })}${Field({ label: `Into fermenter · target ${brew.recipe.batch}`, name: 'actual-volume', value: brew.actuals?.volume ?? '', type: 'number', unit: 'L', step: 0.1 })}</div>${TextArea({ label: 'Brew notes', name: 'actual-notes', value: brew.actuals?.notes ?? '', placeholder: 'What went well? Anything you changed?' })}`, actions: Button({ label: 'Cancel', variant: 'ghost', action: 'close-dialog' }) + Button({ label: 'Save actuals', icon: 'check', action: `save-actuals:${number}` }) }));
}
document.addEventListener('input', event => {
  const input = event.target; const name = input.name; if (!name) return;
  if (name === 'library-search') { picker.query = input.value; updateCatalogueResults(); return; }
  if (name === 'library-amount') { picker.amount = input.value === '' ? '' : Number(input.value); const error = document.querySelector('#catalogue-error'); if (error) error.innerHTML = ''; return; }
  if (name.startsWith('library-')) return;
  if (name === 'recipe-search') { const card = document.querySelector('#library-results'); if (card) card.hidden = !saved.recipe.title.toLowerCase().includes(input.value.toLowerCase()); return; }
  if (name.startsWith('demo-') || name.startsWith('profile-') || name.startsWith('actual-') || name === 'catalogue-choice') return;
  const grain = recipe.grains.find(g => name === `grain-${g.id}-amount`);
  const hop = recipe.hops.find(h => name === `hop-${h.id}-amount` || name === `hop-${h.id}-time`);
  if (grain) grain.kg = input.value === '' ? '' : Number(input.value);
  else if (hop) hop[name.endsWith('-time') ? 'minutes' : 'grams'] = input.value === '' ? '' : Number(input.value);
  else if (Object.hasOwn(recipe, name) && !['grains', 'hops'].includes(name)) recipe[name] = input.type === 'number' ? (input.value === '' ? '' : Number(input.value)) : input.value;
  else return;
  failure = ''; changeState();
});
document.addEventListener('change', event => {
  if (['equipment', 'yeast', 'style'].includes(event.target.name)) {
    recipe[event.target.name] = event.target.value;
    if (event.target.name === 'yeast') { recipe.attenuation = recipe.yeast === 's23' ? 82 : 80; const control = document.querySelector('[name="attenuation"]'); if (control) control.value = recipe.attenuation; }
    changeState();
  }
});
document.addEventListener('click', async event => {
  if (event.target.closest('.skip-link')) { event.preventDefault(); document.querySelector('#main')?.focus(); return; }
  const target = event.target.closest('[data-action]'); if (!target || target.disabled) return;
  event.preventDefault(); const action = target.dataset.action;
  if (action.startsWith('navigate:')) return requestNavigation(action.slice(9));
  if (action.startsWith('tab:demo-')) { selectedSystemTab = action.slice(9); render(); return; }
  if (action.startsWith('tab:')) return requestNavigation(action.slice(4));
  if (action.startsWith('demo:')) return toast(action.slice(5));
  if (action.startsWith('add:')) return openAdd(action.slice(4));
  if (action.startsWith('select-catalogue:')) { picker.selectedId = action.slice('select-catalogue:'.length); updateCatalogueResults(); document.querySelector('#catalogue-results .k-catalogue-item[aria-pressed="true"]')?.focus({ preventScroll: true }); return; }
  if (action.startsWith('catalogue-filter:')) { picker.filter = action.slice('catalogue-filter:'.length); const entries = searchFermentables(allFermentables(), picker.query, picker.filter); if (!entries.some(entry => entry.catalogueId === picker.selectedId)) picker.selectedId = null; openFermentablePicker(); return; }
  if (action.startsWith('remove:')) { const [, kind, id] = action.split(':'); if (id.startsWith('demo-')) return toast('The same ingredient row component powers the editor.'); if (kind === 'grain') recipe.grains = recipe.grains.filter(g => g.id !== id); else recipe.hops = recipe.hops.filter(h => h.id !== id); changeState(); render(); return; }
  if (action.startsWith('inspect:')) return inspectRevision(Number(action.slice(8)));
  if (action.startsWith('restore:')) { const item = history.find(h => h.revision === Number(action.slice(8))); if (!item) return; recipe = clone(item.recipe); changeState(); commitNavigation('recipe'); toast(`Revision ${item.revision} copied into your draft. Save to create a new revision.`); return; }
  if (action.startsWith('actuals:')) return actualsDialog(Number(action.slice(8)));
  if (action.startsWith('save-actuals:')) { const brew = brews.find(b => b.number === Number(action.slice(13))); const values = {}; for (const key of ['og', 'fg', 'volume']) { const value = document.querySelector(`[name="actual-${key}"]`).value; if (value !== '' && (!Number.isFinite(Number(value)) || Number(value) < 0)) return toast('Actual values must be positive, or left blank.'); values[key] = value === '' ? null : Number(value); } values.notes = document.querySelector('[name="actual-notes"]').value; brew.actuals = values; try { localStorage.setItem(BREWS, JSON.stringify(brews)); } catch { return toast('Could not save the actuals.'); } closeDialog(); render(); return toast('Actuals saved in this preview. Original targets are unchanged.'); }
  switch (action) {
    case 'create-fermentable': createFermentableDialog(); break;
    case 'back-to-catalogue': openFermentablePicker(); break;
    case 'save-fermentable': saveCustomFermentable(); break;
    case 'add-selected-fermentable': {
      const item = allFermentables().find(entry => entry.catalogueId === picker.selectedId);
      if (!item) return;
      if (picker.amount === '' || !Number.isFinite(Number(picker.amount)) || Number(picker.amount) <= 0) { document.querySelector('#catalogue-error').innerHTML = Notice({ title: 'Check the amount', body: 'Enter a recipe amount greater than zero kilograms.', tone: 'warning' }); return; }
      recipe.grains.push(resolveFermentable(item, Number(picker.amount), newId()));
      closeDialog(); changeState(); render(); toast(`${item.name} added to your recipe.`); break;
    }
    case 'save': if (await save()) render(); break;
    case 'theme': { const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark'; document.documentElement.dataset.theme = next; try { localStorage.setItem('krausen.design.theme', next); } catch {} render(); break; }
    case 'close-dialog': closeDialog(); pending = null; break;
    case 'demo-dialog': unsavedDialog(null, true); break;
    case 'demo-dismiss': closeDialog(); toast('Your work stays yours.'); break;
    case 'save-leave': await continuePending(); break;
    case 'discard-leave': await continuePending(true); break;
    case 'confirm-add': { const item = catalogue[addKind][Number(document.querySelector('[name="catalogue-choice"]').value)]; if (!item) break; const row = { ...item, id: crypto.randomUUID?.() ?? Array.from(crypto.getRandomValues(new Uint8Array(16)), byte => byte.toString(16).padStart(2, '0')).join('') }; if (addKind === 'grain') recipe.grains.push(row); else recipe.hops.push(row); closeDialog(); changeState(); render(); toast(`${row.name} added.`); break; }
    case 'equipment': equipmentDialog(); break;
    case 'profile-save': { const p = equipment[recipe.equipment]; const keys = ['efficiency', 'boiloff', 'losses', 'absorption']; if (keys.some(key => document.querySelector(`[name="profile-${key}"]`).value === '')) return toast('Complete all equipment settings before updating.'); const values = Object.fromEntries(keys.map(key => [key, Number(document.querySelector(`[name="profile-${key}"]`).value)])); if (Object.values(values).some(v => !Number.isFinite(v) || v < 0) || values.efficiency <= 0 || values.efficiency > 100) return toast('Use valid positive equipment settings.'); const updated = { ...p, ...values, revision: p.revision + 1 }; try { localStorage.setItem('krausen.design.profiles.v1', JSON.stringify({ ...equipment, [recipe.equipment]: updated })); } catch { return toast('Could not save the equipment profile.'); } equipment[recipe.equipment] = updated; closeDialog(); recalculate(); render(); toast('Profile updated for future calculations. Existing brews are unchanged.'); break; }
    case 'start-brew': startBrew(); break;
    case 'mobile-nav': showDialog(Dialog({ id: 'navigation-dialog', title: 'Your brewery', body: `<div class="mobile-nav-links">${Button({ label: 'Recipes', icon: 'recipe', variant: 'ghost', action: 'navigate:library' })}${Button({ label: 'Brews', icon: 'flask', variant: 'ghost', action: 'navigate:brews' })}${Button({ label: 'Design system', icon: 'palette', variant: 'ghost', action: 'navigate:system' })}${Button({ label: 'Toggle appearance', icon: 'moon', variant: 'ghost', action: 'theme' })}</div>` })); break;
  }
});
document.addEventListener('keydown', event => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') { event.preventDefault(); if (document.querySelector('dialog[open]')) return; if (runtimeReady && dirty) save().then(ok => { if (ok) render(); }); }
  if (event.target.matches('[role="tab"]') && ['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) {
    event.preventDefault(); const tabs = [...event.target.closest('[role="tablist"]').querySelectorAll('[role="tab"]')]; const index = tabs.indexOf(event.target); const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length; tabs[next].focus();
  }
});
window.addEventListener('beforeunload', event => { if (dirty) { event.preventDefault(); event.returnValue = ''; } });
window.addEventListener('popstate', () => {
  const target = location.hash.slice(1) || 'recipe'; if (target === route) return;
  if (dirty) { window.history.pushState({ route }, '', `#${route}`); unsavedDialog({ type: 'navigate', target }); }
  else { route = ['recipe', 'system', 'library', 'history', 'brews'].includes(target) ? target : 'recipe'; render(); }
});
window.addEventListener('storage', event => {
  if (event.key !== STORE || !event.newValue) return;
  if (dirty) { failure = 'Another tab saved a newer revision. Your draft is kept.'; updateSaveState(); toast(failure); }
  else { saved = read(STORE, saved); recipe = clone(saved.recipe); history = read(HISTORY, history); render(); }
});
try { await init(); runtimeReady = true; render(); }
catch (error) { document.querySelector('#app').innerHTML = `<main class="page">${Notice({ title: 'The preview needs its calculation bridge', body: 'Run ./build.sh in docs/design/prototype, then serve this folder over HTTP.', tone: 'warning' })}<p>${e(String(error))}</p></main>`; }
