import { expect, type Page } from '@playwright/test';

/**
 * Press Tab `steps` times inside a modal task and assert real containment.
 *
 * Focus may leave the document (activeElement becomes body) only on the Tab
 * pressed while the container's last tabbable control is focused: the browser's
 * own UI follows it. It may then stay out of the document for further Tab
 * presses, and the first element focused afterwards must be inside the
 * container again. Body while a non-boundary control was focused (a removed
 * control, a focus loss) or any element outside the container fails.
 */
export async function expectTabContainment(page: Page, containerSelector: string, steps: number) {
  type Step = { inside: boolean; body: boolean; wasLast: boolean; label: string };
  let left = false;
  for (let index = 0; index < steps; index++) {
    await page.evaluate((selector) => {
      const container = document.querySelector(selector)!;
      const tabbable = [...container.querySelectorAll<HTMLElement>(
        'a[href], button, input, select, textarea, [tabindex]',
      )].filter((el) => el.tabIndex >= 0 && !el.matches(':disabled') && el.getClientRects().length > 0 && !el.closest('[inert]'));
      (window as unknown as { __previous?: Element | null; __last?: Element | null }).__previous = document.activeElement;
      (window as unknown as { __last?: Element | null }).__last = tabbable[tabbable.length - 1] ?? null;
    }, containerSelector);
    await page.keyboard.press('Tab');
    const step: Step = await page.evaluate((selector) => {
      const w = window as unknown as { __previous?: Element | null; __last?: Element | null };
      const container = document.querySelector(selector)!;
      const active = document.activeElement;
      const body = !active || active === document.body;
      return {
        inside: !!active && container.contains(active),
        body,
        wasLast: !!w.__previous && w.__previous === w.__last,
        label: body ? 'body' : `${active!.tagName}#${(active as HTMLElement).id} ${(active!.textContent || '').slice(0, 24)}`,
      };
    }, containerSelector);
    if (step.inside) {
      left = false;
    } else if (step.body) {
      expect(left || step.wasLast, `focus fell to the document body at step ${index + 1} without leaving through the last control`).toBe(true);
      left = true;
    } else {
      expect(step.inside, `focus reached ${step.label} outside ${containerSelector} at step ${index + 1}`).toBe(true);
    }
  }
}
