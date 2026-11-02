import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const ui = new URL('../../crates/krausen-ui/', import.meta.url);
const layout = readFileSync(new URL('src/pages/auth/auth_layout.rs', ui), 'utf8');

test('desktop and mobile auth layouts use the approved shared barley brand', () => {
  assert.equal((layout.match(/<BreweryBrand(?:\s+inverse=true)?\s*\/>/g) ?? []).length, 2,
    'desktop and mobile must both render BreweryBrand');
  assert.doesNotMatch(layout, /krausen_full_logo(?:_white)?\.svg/,
    'auth must not load the purple K placeholder assets');
  const brand = readFileSync(new URL('src/components/brewery_kit.rs', ui), 'utf8');
  assert.match(brand, /pub fn BreweryBrand\([\s\S]*?src="\/public\/brewery-mark\.svg"/);
  assert.match(layout, /<BreweryBrand inverse=true\/>/,
    'the dark desktop panel needs the inverse wordmark');
  const css = readFileSync(new URL('style/kit.css', ui), 'utf8');
  assert.match(css, /\.k-brand--inverse\s*\{\s*color:\s*var\(--k-color-auth-foreground\)/);
  const tokens = readFileSync(new URL('style/approved-tokens.css', ui), 'utf8');
  assert.match(tokens, /--k-color-auth-foreground:\s*var\(--k-color-brand-foreground\)/);
  assert.match(tokens, /\.dark\s*\{[\s\S]*?--k-color-auth-foreground:\s*var\(--k-color-foreground\)/);
});
