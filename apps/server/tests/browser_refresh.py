# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise concurrent tabs with a real shared browser cookie jar."""
import asyncio
import json
import sys
from playwright.async_api import async_playwright

async def main():
    data = json.load(sys.stdin)
    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True, args=["--no-sandbox"])
        context = await browser.new_context()
        await context.add_cookies([{"name": "refresh_token", "value": data["refresh"], "url": data["base"], "httpOnly": True, "sameSite": "Lax"}])
        attempts = 0
        both_waiting = asyncio.Event()
        def record_attempt(_source):
            nonlocal attempts
            attempts += 1
            if attempts >= 2:
                both_waiting.set()
        await context.expose_binding("recordRefreshAttempt", record_attempt)
        await context.add_init_script("""(() => {
            const request = navigator.locks.request.bind(navigator.locks);
            navigator.locks.request = (...args) => { window.recordRefreshAttempt(); return request(...args); };
        })()""")
        async def hold_first(route):
            await asyncio.wait_for(both_waiting.wait(), timeout=10)
            await route.continue_()
        await context.route("**/api/v1/auth/refresh", hold_first)
        first = await context.new_page()
        second = await context.new_page()
        await asyncio.gather(first.goto(data["base"] + "/test-refresh"), second.goto(data["base"] + "/test-refresh"))
        assert await first.evaluate("typeof window.krausenRefresh === 'function'")
        assert await second.evaluate("typeof window.krausenRefresh === 'function'")
        # An additional coordinated refresh proves neither earlier request revoked the family.
        assert await first.evaluate("window.krausenRefresh()")
        no_locks = await context.new_page()
        await no_locks.add_init_script("Object.defineProperty(navigator, 'locks', {value: undefined})")
        await no_locks.goto(data["base"] + "/test-refresh")
        assert not await no_locks.evaluate("window.krausenRefresh()")
        await browser.close()

asyncio.run(asyncio.wait_for(main(), timeout=45))
