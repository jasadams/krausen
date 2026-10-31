// SPDX-License-Identifier: AGPL-3.0-or-later
// The prototype's shared component layer. Screens compose these components;
// tokens.css and components.css own every visual variant. Content slots
// (Panel/Dialog body, actions, footer) accept trusted component markup only.

export function escapeHtml(value = '') {
  return String(value ?? '').replace(/[&<>"']/g, character => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  })[character]);
}

const e = escapeHtml;

// Pass attributes as an object where possible. The small string adapter is
// for static screen markup; values are still escaped, and event/style/URL
// attributes are deliberately unsupported.
function attributes(input = '') {
  const entries = typeof input === 'object' && input !== null
    ? Object.entries(input)
    : Array.from(String(input).matchAll(/([\w:-]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'<>`=]+)))?/g))
      .map(match => [match[1], match[2] ?? match[3] ?? match[4] ?? true]);
  return entries.filter(([name, value]) => (
    /^(?:aria-[\w-]+|data-[\w-]+|id|title|role|tabindex|autocomplete|inputmode|max|maxlength|minlength|pattern|required|readonly|autofocus|multiple|form|spellcheck)$/.test(name)
    && value !== false && value !== undefined && value !== null
  )).map(([name, value]) => value === true ? ` ${name}` : ` ${name}="${e(value)}"`).join('');
}

const paths = {
  leaf: '<path d="M20 4c-9-1-16 2-16 9a7 7 0 0 0 7 7c7 0 10-7 9-16Z"/><path d="m4 20 11-11"/>',
  grain: '<path d="M12 21V3M12 8C7 8 6 5 6 3c4 0 6 2 6 5Zm0 6c-5 0-6-3-6-5 4 0 6 2 6 5Zm0 6c-5 0-6-3-6-5 4 0 6 2 6 5Zm0-12c5 0 6-3 6-5-4 0-6 2-6 5Zm0 6c5 0 6-3 6-5-4 0-6 2-6 5Zm0 6c5 0 6-3 6-5-4 0-6 2-6 5Z"/>',
  hops: '<path d="M12 3c-3 2-7 6-7 10a7 7 0 0 0 14 0c0-4-4-8-7-10Z"/><path d="m6 9 6 4 6-4M5 14l7 4 7-4M12 3v18"/>',
  recipe: '<path d="M7 3h11a2 2 0 0 1 2 2v16H7a3 3 0 0 1-3-3V6a3 3 0 0 1 3-3Zm0 0v12M4 18a3 3 0 0 1 3-3h13M11 7h5M11 10h5"/>',
  flask: '<path d="M9 3h6M10 3v6l-6 10a1.3 1.3 0 0 0 1 2h14a1.3 1.3 0 0 0 1-2L14 9V3M7 14h10"/>',
  beaker: '<path d="M5 3h14M7 3v16a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2V3M7 13h10M13 7h4M13 10h4"/>',
  settings: '<path d="m9 3-1 3-3 1-2 3 2 2-1 3 2 3 3-1 3 2 3-2 3 1 2-3-1-3 2-2-2-3-3-1-1-3H9Z"/><circle cx="12" cy="11" r="3"/>',
  'chevron-down': '<path d="m6 9 6 6 6-6"/>',
  left: '<path d="m14 6-6 6 6 6"/>',
  right: '<path d="m10 6 6 6-6 6"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  close: '<path d="m6 6 12 12M18 6 6 18"/>',
  check: '<path d="m5 12 4 4L19 6"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  history: '<path d="M3 11a9 9 0 1 1 2 7M3 4v7h7M12 7v5l3 2"/>',
  save: '<path d="M5 3h12l4 4v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Zm2 0v6h10V3M7 21v-8h10v8"/>',
  'arrow-up-right': '<path d="M6 18 18 6M6 6h12v12"/>',
  'arrow-right': '<path d="M4 12h16m-6-6 6 6-6 6"/>',
  search: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1"/>',
  moon: '<path d="M21 13a9 9 0 0 1-10-10 9 9 0 1 0 10 10Z"/>',
  menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
  more: '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
  trash: '<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7"/>',
  undo: '<path d="M3 5v6h6M3 11a8 8 0 1 1 2 7"/>',
  water: '<path d="M12 2S5 10 5 15a7 7 0 0 0 14 0c0-5-7-13-7-13Z"/><path d="M9 15a3 3 0 0 0 3 3"/>',
  thermometer: '<path d="M9 14V5a3 3 0 0 1 6 0v9a5 5 0 1 1-6 0Z"/><path d="M12 8v9"/><circle cx="12" cy="18" r="1"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/>',
  warning: '<path d="m12 3 10 18H2L12 3Z"/><path d="M12 9v5M12 17h.01"/>',
  download: '<path d="M12 3v12m-5-5 5 5 5-5M4 15v6h16v-6"/>',
  folder: '<path d="M3 5h6l2 2h10v12H3V5Z"/>',
  palette: '<path d="M12 3a9 9 0 1 0 0 18h1a2 2 0 0 0 1-4 2 2 0 0 1 1-3h2a4 4 0 0 0 4-4c0-4-4-7-9-7Z"/><circle cx="7" cy="10" r=".7"/><circle cx="11" cy="7" r=".7"/><circle cx="16" cy="8" r=".7"/>',
  grid: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>',
  eye: '<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M4 21v-2a8 8 0 0 1 16 0v2"/>',
};

export function icon(name = 'info', size = 18) {
  const px = Math.max(10, Math.min(64, Number(size) || 18));
  return `<svg class="k-icon" width="${px}" height="${px}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">${paths[name] ?? paths.info}</svg>`;
}

export function Button({ label = '', icon: iconName = '', variant = 'primary', size = 'md', action = '', disabled = false, attrs = '', className = '' } = {}) {
  const hasAccessibleLabel = typeof attrs === 'object' && attrs !== null
    ? Boolean(attrs['aria-label'] || attrs['aria-labelledby'])
    : /\baria-label(?:ledby)?\s*=/.test(String(attrs));
  const extra = !label && iconName && !hasAccessibleLabel ? { 'aria-label': iconName } : {};
  return `<button type="button" class="k-button k-button--${e(variant)} k-button--${e(size)} ${e(className)}"${action ? ` data-action="${e(action)}"` : ''}${disabled ? ' disabled' : ''}${attributes(extra)}${attributes(attrs)}>${iconName ? icon(iconName) : ''}${label ? `<span>${e(label)}</span>` : ''}</button>`;
}

function fieldShell({ label, name, hint, error, className = '', control }) {
  const id = `field-${name}`;
  return `<div class="k-field ${error ? 'k-field--error' : ''} ${e(className)}">${label ? `<label class="k-field__label" for="${e(id)}">${e(label)}</label>` : ''}${control(id)}${error ? `<span class="k-field__error" id="${e(id)}-error">${icon('warning', 14)}${e(error)}</span>` : ''}${hint ? `<span class="k-field__hint" id="${e(id)}-hint">${e(hint)}</span>` : ''}</div>`;
}

export function Field({ label = '', name = '', value = '', type = 'text', unit = '', hint = '', error = '', min, step, placeholder = '', variant = 'default', className = '', attrs = '' } = {}) {
  const acceptedType = ['text', 'number', 'email', 'search', 'password', 'date', 'datetime-local'].includes(type) ? type : 'text';
  return fieldShell({ label, name, hint, error, className: `k-field--${variant} ${className}`, control: id => `<div class="k-field__control${unit ? ' k-field__control--unit' : ''}"><input id="${e(id)}" name="${e(name)}" type="${acceptedType}" value="${e(value)}" placeholder="${e(placeholder)}"${min !== undefined ? ` min="${e(min)}"` : ''}${step !== undefined ? ` step="${e(step)}"` : ''}${error ? ' aria-invalid="true"' : ''}${hint || error ? ` aria-describedby="${e(id)}-${error ? 'error' : 'hint'}"` : ''}${attributes(attrs)}>${unit ? `<span class="k-field__unit" aria-hidden="true">${e(unit)}</span>` : ''}</div>` });
}

export function Select({ label = '', name = '', value = '', options = [], hint = '', className = '', attrs = '' } = {}) {
  return fieldShell({ label, name, hint, className, control: id => `<div class="k-field__control k-select"><select id="${e(id)}" name="${e(name)}"${hint ? ` aria-describedby="${e(id)}-hint"` : ''}${attributes(attrs)}>${options.map(option => {
    const item = typeof option === 'string' ? { value: option, label: option } : option;
    return `<option value="${e(item.value)}"${String(item.value) === String(value) ? ' selected' : ''}${item.disabled ? ' disabled' : ''}>${e(item.label)}</option>`;
  }).join('')}</select>${icon('chevron-down', 15)}</div>` });
}

export function TextArea({ label = '', name = '', value = '', placeholder = '', rows = 3, attrs = '' } = {}) {
  return fieldShell({ label, name, control: id => `<div class="k-field__control"><textarea id="${e(id)}" name="${e(name)}" rows="${Math.max(1, Math.min(24, Number(rows) || 3))}" placeholder="${e(placeholder)}"${attributes(attrs)}>${e(value)}</textarea></div>` });
}

export function Badge({ label = '', tone = 'neutral', icon: iconName = '' } = {}) {
  return `<span class="k-badge k-badge--${e(tone)}">${iconName ? icon(iconName, 13) : ''}${e(label)}</span>`;
}

export function Panel({ title = '', eyebrow = '', description = '', actions = '', body = '', footer = '', className = '' } = {}) {
  return `<section class="k-panel ${e(className)}">${title || eyebrow || description || actions ? `<header class="k-panel__header"><div>${eyebrow ? `<span class="k-panel__eyebrow">${e(eyebrow)}</span>` : ''}${title ? `<h2 class="k-panel__title">${e(title)}</h2>` : ''}${description ? `<p class="k-panel__description">${e(description)}</p>` : ''}</div>${actions ? `<div class="k-panel__actions">${actions}</div>` : ''}</header>` : ''}<div class="k-panel__body">${body}</div>${footer ? `<footer class="k-panel__footer">${footer}</footer>` : ''}</section>`;
}

function finiteNumber(value) {
  if (value === null || value === undefined || typeof value === 'boolean'
    || !['number', 'string'].includes(typeof value)
    || typeof value === 'string' && !value.trim()) return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function rangeNumber(number, digits) {
  return new Intl.NumberFormat('en-AU', { minimumFractionDigits: digits, maximumFractionDigits: digits, useGrouping: false }).format(number);
}

function estimatePrecision(value, min, max, digits) {
  const current = finiteNumber(value), lower = finiteNumber(min), upper = finiteNumber(max);
  if (current === null || lower === null || upper === null) return digits;
  const boundary = current < lower - 1e-10 ? lower : current > upper + 1e-10 ? upper : null;
  if (boundary === null) return digits;
  let precision = digits;
  // Reveal a difference that normal display rounding would conceal.
  while (precision < 6 && rangeNumber(current, precision) === rangeNumber(boundary, precision)) precision++;
  return precision;
}

export function StyleRange({ label = '', value, min, max, unit = '', digits = 1, compact = false } = {}) {
  const current = finiteNumber(value);
  const lower = finiteNumber(min);
  const upper = finiteNumber(max);
  const precision = Math.max(0, Math.min(6, Math.trunc(Number(digits) || 0)));
  const hasReference = lower !== null && upper !== null && lower < upper;
  const hasEstimate = current !== null && hasReference;
  const state = !hasEstimate ? 'awaiting'
    : current < lower - 1e-10 ? 'below'
      : current > upper + 1e-10 ? 'above' : 'within';
  const status = { awaiting: 'Awaiting estimate', below: 'Below style', within: 'Within style', above: 'Above style' }[state];
  // The style band stays between 25% and 75%. Only the estimate marker moves.
  // The visible tails make values outside the reference range understandable.
  const position = hasEstimate ? Math.max(2, Math.min(98, 25 + (current - lower) / (upper - lower) * 50)) : null;
  const display = number => number === null ? '—' : rangeNumber(number, precision);
  const suffix = unit ? ` ${unit}` : '';
  const limitsSuffix = compact ? '' : suffix;
  const reference = hasReference ? `Style reference ${display(lower)} to ${display(upper)}${suffix}.` : 'Style reference unavailable.';
  const accessible = `${label ? `${label}. ` : ''}${hasEstimate ? `Estimate ${rangeNumber(current, estimatePrecision(current, lower, upper, precision))}${suffix}. ` : ''}${reference} ${status}.`;
  return `<div class="k-style-range k-style-range--${state}${compact ? ' k-style-range--compact' : ''}" role="img" aria-label="${e(accessible)}" data-style-state="${state}"${current !== null ? ` data-value="${current}"` : ''}${lower !== null ? ` data-min="${lower}"` : ''}${upper !== null ? ` data-max="${upper}"` : ''}>
    <div class="k-style-range__track" aria-hidden="true"><span class="k-style-range__band"></span>${position !== null ? `<span class="k-style-range__marker" style="--style-range-position:${position}%"></span>` : ''}</div>
    <div class="k-style-range__limits" aria-hidden="true"><span>${e(display(lower) + limitsSuffix)}</span><span>${e(display(upper) + limitsSuffix)}</span></div>
    <span class="k-style-range__status" aria-hidden="true">${e(status)}</span>
  </div>`;
}

export function Metric({ label = '', value = '—', unit = '', hint = '', tone = '', id = '', range = null } = {}) {
  const estimate = range && Object.hasOwn(range, 'value') ? range.value : value;
  const number = finiteNumber(value);
  const precision = Math.max(0, Math.min(6, Math.trunc(Number(range?.digits ?? 1) || 0)));
  const expandedPrecision = range ? estimatePrecision(estimate, range.min, range.max, precision) : precision;
  const displayedValue = range && expandedPrecision > precision && finiteNumber(estimate) !== null ? rangeNumber(finiteNumber(estimate), expandedPrecision)
    : range && number === null ? '—' : range && typeof value === 'number' ? number.toFixed(precision) : value;
  return `<div class="k-metric${tone ? ` k-metric--${e(tone)}` : ''}"${id ? ` id="${e(id)}"` : ''}><div class="k-metric__label">${e(label)}</div><div class="k-metric__value"><span data-metric-value>${e(displayedValue)}</span>${unit ? `<span class="k-metric__unit">${e(unit)}</span>` : ''}</div>${range ? StyleRange({ label, value: estimate, min: range.min, max: range.max, unit, digits: range.digits ?? 1, compact: tone === 'compact' }) : ''}${hint ? `<div class="k-metric__hint">${e(hint)}</div>` : ''}</div>`;
}

export function EmptyState({ title = '', description = '', action = '' } = {}) {
  return `<div class="k-empty-state"><div class="k-empty-state__icon">${icon('flask', 26)}</div><h3 class="k-empty-state__title">${e(title)}</h3>${description ? `<p class="k-empty-state__description">${e(description)}</p>` : ''}${action ? `<div class="k-empty-state__action">${action}</div>` : ''}</div>`;
}

export function Notice({ title = '', body = '', tone = 'info', icon: iconName = '' } = {}) {
  return `<div class="k-notice k-notice--${e(tone)}" role="${tone === 'error' ? 'alert' : 'status'}"><span class="k-notice__icon">${icon(iconName || (tone === 'warning' || tone === 'error' ? 'warning' : tone === 'success' ? 'check' : 'info'), 18)}</span><div>${title ? `<div class="k-notice__title">${e(title)}</div>` : ''}${body ? `<div class="k-notice__body">${e(body)}</div>` : ''}</div></div>`;
}

export function Tabs({ items = [], active = '' } = {}) {
  return `<div class="k-tabs" role="tablist" aria-label="Recipe sections">${items.map(item => `<button type="button" class="k-tab${item.id === active ? ' is-active' : ''}" role="tab" aria-selected="${item.id === active}" tabindex="${item.id === active ? '0' : '-1'}" data-action="tab:${e(item.id)}"${item.disabled ? ' disabled' : ''}><span>${e(item.label)}</span>${item.count !== undefined ? `<span class="k-tab__count">${e(item.count)}</span>` : ''}</button>`).join('')}</div>`;
}

function ingredientSwatch(colour) {
  const tone = ['pale', 'amber', 'copper', 'dark'].includes(colour) ? colour
    : Number(colour) >= 100 ? 'dark' : Number(colour) >= 30 ? 'copper' : Number(colour) >= 10 ? 'amber' : 'pale';
  return `<span class="k-ingredient-row__swatch k-ingredient-row__swatch--${tone}" aria-hidden="true"></span>`;
}

export function IngredientRow({ id = '', name = '', subtitle = '', amount = '', unit = 'kg', percent = '', colour = '', kind = 'grain', time = 60, alpha = '', editable = true } = {}) {
  const rowKind = kind === 'hops' ? 'hop' : ['grain', 'hop', 'yeast'].includes(kind) ? kind : 'grain';
  const fieldName = `${rowKind}-${id}-amount`;
  const iconName = rowKind === 'hop' ? 'hops' : rowKind === 'yeast' ? 'flask' : 'grain';
  const description = [subtitle, alpha !== '' ? `${alpha}${String(alpha).includes('%') ? '' : '%'} α-acid` : ''].filter(Boolean).join(' · ');
  const amountControl = editable
    ? `<input type="number" name="${e(fieldName)}" aria-label="${e(name)} amount in ${e(unit)}" value="${e(amount)}" min="0" step="${rowKind === 'grain' ? '0.01' : '1'}" inputmode="decimal">`
    : `<span class="k-ingredient-row__value">${e(amount)}</span>`;
  const timeControl = editable
    ? `<input type="number" name="hop-${e(id)}-time" aria-label="${e(name)} boil time in minutes" value="${e(time)}" min="0" step="1" inputmode="numeric">`
    : `<span class="k-ingredient-row__value">${e(time)}</span>`;
  const share = percent === '' ? '' : `${percent}${String(percent).includes('%') ? '' : '%'}`;
  return `<div class="k-ingredient-row k-ingredient-row--${rowKind}" data-ingredient-id="${e(id)}"><div class="k-ingredient-row__identity"><span class="k-ingredient-row__icon">${rowKind === 'grain' ? ingredientSwatch(colour) : icon(iconName, 21)}</span><div><div class="k-ingredient-row__name">${e(name)}</div>${description ? `<div class="k-ingredient-row__subtitle">${e(description)}</div>` : ''}</div></div><div class="k-ingredient-row__amount">${amountControl}<span class="k-ingredient-row__unit" aria-hidden="true">${e(unit)}</span></div>${rowKind === 'hop' ? `<div class="k-ingredient-row__time">${timeControl}<span class="k-ingredient-row__unit" aria-hidden="true">min</span></div>` : `<div class="k-ingredient-row__percent">${e(share)}</div>`}<div class="k-ingredient-row__actions">${editable ? Button({ label: '', icon: 'close', variant: 'ghost', size: 'icon', action: `remove:${rowKind}:${id}`, attrs: { 'aria-label': `Remove ${name}` } }) : ''}</div></div>`;
}

export function CatalogueItem({ id = '', name = '', producer = '', category = '', colour, yieldPercent, source = 'starter', selected = false, action = '' } = {}) {
  const ebc = finiteNumber(colour);
  const extractYield = finiteNumber(yieldPercent);
  const format = value => value.toFixed(1).replace(/\.0$/, '');
  const specifications = [
    ebc !== null ? `${format(ebc)} EBC` : '',
    extractYield !== null ? `${format(extractYield)}% yield` : '',
  ].filter(Boolean);
  const library = source !== 'starter';
  return `<button type="button" class="k-catalogue-item${selected ? ' k-catalogue-item--selected' : ''}" data-catalogue-id="${e(id)}" data-action="${e(action || `select-catalogue:${id}`)}" aria-pressed="${Boolean(selected)}">
    <span class="k-catalogue-item__identity"><span class="k-catalogue-item__icon" aria-hidden="true">${icon('grain', 22)}</span><span class="k-catalogue-item__text"><span class="k-catalogue-item__name">${e(name)}</span>${producer ? `<span class="k-catalogue-item__producer">${e(producer)}</span>` : ''}${category ? `<span class="k-catalogue-item__category">${e(category)}</span>` : ''}</span></span>
    <span class="k-catalogue-item__meta"><span class="k-catalogue-item__specs">${specifications.length ? specifications.map(specification => `<span>${e(specification)}</span>`).join('') : '<span class="k-catalogue-item__unspecified">Specifications not provided</span>'}</span>${Badge({ label: library ? 'Your library' : 'Starter catalogue', tone: library ? 'accent' : 'neutral' })}</span>
    <span class="k-catalogue-item__selection" aria-hidden="true">${selected ? icon('check', 17) : ''}</span>
  </button>`;
}

export function Dialog({ id = '', title = '', description = '', body = '', actions = '', wide = false } = {}) {
  return `<dialog class="k-dialog${wide ? ' k-dialog--wide' : ''}" id="${e(id)}" aria-labelledby="${e(id)}-title"${description ? ` aria-describedby="${e(id)}-description"` : ''}><header class="k-dialog__header"><div><h2 id="${e(id)}-title">${e(title)}</h2>${description ? `<p id="${e(id)}-description">${e(description)}</p>` : ''}</div>${Button({ icon: 'close', variant: 'ghost', size: 'icon', action: 'close-dialog', attrs: { 'aria-label': 'Close dialog' } })}</header>${body ? `<div class="k-dialog__body">${body}</div>` : ''}${actions ? `<footer class="k-dialog__actions">${actions}</footer>` : ''}</dialog>`;
}

export function SaveStatus({ state = 'saved', text = '' } = {}) {
  const labels = { saved: 'All changes saved', dirty: 'Unsaved changes', saving: 'Saving…', error: 'Could not save', conflict: 'Newer revision available', recovered: 'Draft recovered' };
  return `<span class="k-save-status k-save-status--${e(state)}" role="status" aria-live="polite">${state === 'saved' ? icon('check', 14) : state === 'error' || state === 'conflict' ? icon('warning', 14) : `<span class="k-save-status__dot" aria-hidden="true"></span>`}<span>${e(text || labels[state] || labels.dirty)}</span></span>`;
}
