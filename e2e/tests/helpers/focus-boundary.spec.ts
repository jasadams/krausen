import { test, expect, type Page } from '@playwright/test';
import { expectTabContainment } from '../../helpers/focus-boundary';

// The containment helper is acceptance evidence for KRA-43's modal tasks, so it
// is shown to pass a contained task and to fail each kind of escape it claims to
// catch. These pages are static fixtures, not product code.

async function fixture(page: Page, body: string) {
  await page.setContent(`<!doctype html><body><button id="outside-before">before</button>${body}<button id="outside-after">after</button></body>`);
}

test('a contained task passes: Tab and Shift+Tab leave through the boundary and come back inside', async ({ page }) => {
  await fixture(page, '<div id="task"><h2 id="h" tabindex="-1">Task</h2><button>One</button><button>Two</button></div>');
  // The page outside the task is inert, as it is behind a real modal.
  await page.evaluate(() => document.querySelectorAll('#outside-before, #outside-after').forEach((el) => el.setAttribute('inert', '')));
  await page.locator('#task button').first().focus();
  await expectTabContainment(page, '#task', 6);
  await page.locator('#h').focus();
  await expectTabContainment(page, '#task', 6, { shift: true });
});

test('focus reaching an element outside the task fails', async ({ page }) => {
  await fixture(page, '<div id="task"><button>One</button><button>Two</button></div>');
  await page.locator('#task button').first().focus();
  await expect(expectTabContainment(page, '#task', 6)).rejects.toThrow(/outside #task/);
});

test('focus dropping to the body from a mid-task control fails', async ({ page }) => {
  await fixture(page, '<div id="task"><button>One</button><button id="vanishing">Two</button><button>Three</button></div>');
  await page.evaluate(() => document.querySelectorAll('#outside-before, #outside-after').forEach((el) => el.setAttribute('inert', '')));
  // Focusing the middle control immediately loses focus: a control removed or disabled under the user.
  await page.locator('#vanishing').evaluate((el) => el.addEventListener('focus', () => (el as HTMLElement).blur()));
  await page.locator('#task button').first().focus();
  await expect(expectTabContainment(page, '#task', 6)).rejects.toThrow(/without leaving through the boundary control/);
});

test('focus that stays out of the task for more than two presses fails', async ({ page }) => {
  await fixture(page, '<div id="task"><button id="only">Only</button></div>');
  await page.evaluate(() => document.querySelectorAll('#outside-before, #outside-after').forEach((el) => el.setAttribute('inert', '')));
  // After the boundary press the only control becomes unreachable, so focus never comes back.
  await page.locator('#only').evaluate((el) => el.addEventListener('blur', () => { (el as HTMLButtonElement).disabled = true; }));
  await page.locator('#only').focus();
  await expect(expectTabContainment(page, '#task', 4)).rejects.toThrow(/more than 2 presses|never returned/);
});
