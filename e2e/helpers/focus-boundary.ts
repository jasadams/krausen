import { expect, type Page } from '@playwright/test';

/** The longest run of document-body steps Chromium produces while focus is in its own UI. */
const MAX_BODY_RUN = 2;

/**
 * Press Tab (or Shift+Tab) `steps` times inside a modal task and assert real
 * containment.
 *
 * Focus may leave the document (activeElement becomes body) only on the press
 * made while the container's boundary control is focused: its last tabbable for
 * Tab, its first for Shift+Tab. It may then stay out for at most
 * `MAX_BODY_RUN` presses, and the first element focused afterwards must be
 * inside the container. Body from any other control (a removed control, a focus
 * loss), a longer run, an element outside the container, or focus that never
 * comes back inside, fails.
 */
export async function expectTabContainment(
  page: Page,
  containerSelector: string,
  steps: number,
  { shift = false }: { shift?: boolean } = {},
) {
  type Step = { inside: boolean; body: boolean; wasBoundary: boolean; label: string };
  const key = shift ? 'Shift+Tab' : 'Tab';
  const container = page.locator(containerSelector);
  let bodyRun = 0;
  let pressed = 0;
  // Keep pressing after `steps` until focus is observed back inside the container.
  while (pressed < steps || bodyRun > 0) {
    expect(pressed, `focus never returned inside ${containerSelector} after leaving it`).toBeLessThan(steps + MAX_BODY_RUN + 2);
    // The editor may remount a preview while it revalidates after a focus change.
    await container.waitFor();
    await page.evaluate(
      ({ selector, backwards }) => {
        const root = document.querySelector(selector)!;
        const tabbable = [...root.querySelectorAll<HTMLElement>('a[href], button, input, select, textarea, [tabindex]')].filter(
          (el) => el.tabIndex >= 0 && !el.matches(':disabled') && el.getClientRects().length > 0 && !el.closest('[inert]'),
        );
        const w = window as unknown as { __previous?: Element | null; __boundary?: Element | null };
        w.__previous = document.activeElement;
        w.__boundary = (backwards ? tabbable[0] : tabbable[tabbable.length - 1]) ?? null;
      },
      { selector: containerSelector, backwards: shift },
    );
    await page.keyboard.press(key);
    pressed += 1;
    await container.waitFor();
    const step: Step = await page.evaluate((selector) => {
      const w = window as unknown as { __previous?: Element | null; __boundary?: Element | null };
      const root = document.querySelector(selector)!;
      const active = document.activeElement;
      const body = !active || active === document.body;
      return {
        inside: !!active && root.contains(active),
        body,
        wasBoundary: !!w.__previous && w.__previous === w.__boundary,
        label: body ? 'body' : `${active!.tagName}#${(active as HTMLElement).id} ${(active!.textContent || '').slice(0, 24)}`,
      };
    }, containerSelector);
    if (step.inside) {
      bodyRun = 0;
    } else if (step.body) {
      if (bodyRun === 0) {
        expect(step.wasBoundary, `focus fell to the document body at press ${pressed} without leaving through the boundary control`).toBe(true);
      }
      bodyRun += 1;
      expect(bodyRun, `focus stayed out of ${containerSelector} for more than ${MAX_BODY_RUN} presses`).toBeLessThanOrEqual(MAX_BODY_RUN);
    } else {
      expect(step.inside, `focus reached ${step.label} outside ${containerSelector} at press ${pressed}`).toBe(true);
    }
  }
}
