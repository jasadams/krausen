# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise concurrent tabs with a real shared browser cookie jar.

The page pre-warm (`crates/krausen-ui/index.html`) posts to the refresh endpoint
without any cross-tab lock. Two tabs refreshing the same cookie at the same time
must both succeed (the refresh-token grace period covers the overlap), and a tab
with no `navigator.locks` must refresh too (insecure LAN origins have none).
"""
import asyncio
import json
import sys
from playwright.async_api import async_playwright

async def wait_for(condition, timeout=10):
    for _ in range(int(timeout / 0.05)):
        if condition():
            return
        await asyncio.sleep(0.05)
    raise AssertionError("condition not met in time")

async def main():
    data = json.load(sys.stdin)
    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True, args=["--no-sandbox"])
        context = await browser.new_context()
        await context.add_cookies([{"name": "refresh_token", "value": data["refresh"], "url": data["base"], "httpOnly": True, "sameSite": "Strict"}])
        statuses = []
        arrived = 0
        both_arrived = asyncio.Event()
        async def hold_until_both(route):
            nonlocal arrived
            arrived += 1
            if arrived >= 2:
                both_arrived.set()
            await asyncio.wait_for(both_arrived.wait(), timeout=10)
            await route.continue_()
        await context.route("**/api/v1/auth/refresh", hold_until_both)
        def record(response):
            if response.url.endswith("/api/v1/auth/refresh"):
                statuses.append(response.status)
        first = await context.new_page()
        second = await context.new_page()
        first.on("response", record)
        second.on("response", record)
        await asyncio.gather(first.goto(data["base"] + "/test-refresh"), second.goto(data["base"] + "/test-refresh"))
        await wait_for(lambda: len(statuses) >= 2)
        assert statuses == [200, 200], statuses
        # The rotated cookies are usable by a third tab with no Web Locks at all.
        await context.unroute("**/api/v1/auth/refresh")
        no_locks = await context.new_page()
        await no_locks.add_init_script("Object.defineProperty(navigator, 'locks', {value: undefined})")
        no_locks.on("response", record)
        await no_locks.goto(data["base"] + "/test-refresh")
        await wait_for(lambda: len(statuses) >= 3)
        assert statuses == [200, 200, 200], statuses
        probe = await no_locks.evaluate("fetch('/test-probe', {credentials: 'same-origin'}).then(r => r.status)")
        assert probe == 200, probe
        await browser.close()

asyncio.run(asyncio.wait_for(main(), timeout=45))
