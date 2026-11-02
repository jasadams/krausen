#!/usr/bin/env python3
"""Compiler-free branding guard; run alongside the Rust email_service tests.

Checks actual production HTML literals/assets, not a duplicated email model.
No SMTP, network, third-party Python dependencies or credentials required.
"""
import hashlib
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[3]
SOURCE = (ROOT / "crates/krausen-auth/src/email_service.rs").read_text().split("#[cfg(test)]", 1)[0]
TOKENS = (ROOT / "crates/krausen-ui/style/approved-tokens.css").read_text()


class EmailBrandSourceTests(unittest.TestCase):
    def test_no_legacy_purple(self):
        self.assertNotIn("#4f46e5", SOURCE.lower())

    def test_shared_templates_use_approved_colors(self):
        for name in ("email_template", "admin_email_template"):
            template = SOURCE.split(f"fn {name}(", 1)[1].split('\n}\n', 1)[0]
            for color in ("#f7f6f2", "#292c25", "#a34728", "#1c211c", "#f0eee5"):
                with self.subTest(template=name, color=color):
                    self.assertIn(color, template.lower())
                    self.assertIn(color, TOKENS)
            self.assertIn('src="cid:krausen_logo"', template)
            self.assertIn('alt="Krausen"', template)
            self.assertIn('>krausen', template)

    def test_logo_is_approved_barley_raster(self):
        # PNG rasterized from the unchanged production brewery-mark.svg at 3x.
        logo = (ROOT / "assets/krausen_email_logo.png").read_bytes()
        self.assertEqual(logo[:8], b'\x89PNG\r\n\x1a\n')
        self.assertEqual(hashlib.sha256(logo).hexdigest(),
                         "6353ffdc4ea7ce8bb917c4d80a03d481835ca43d571eb60085641701e743f066")

    def test_ownership_uses_shared_brand_wrapper(self):
        method = SOURCE.split("pub async fn send_ownership_transfer(", 1)[1].split(
            "pub async fn send_passkey_recovery(", 1)[0]
        self.assertIn('email_template(', method)

    def test_all_message_links_expiry_and_escaping_contracts(self):
        methods = dict(re.findall(
            r'pub async fn (send_\w+)\((.*?)(?=pub async fn |\n}\n)', SOURCE, re.S))
        expected = {
            "send_workspace_invitation": (["{frontend_url}/login",
                "{frontend_url}/unsubscribe?email={escaped_email}",
                "{frontend_url}/privacy", "{frontend_url}/terms", "{frontend_url}"],
                "This invitation will expire in 7 days."),
            "send_ownership_transfer": (["{frontend_url}/settings/team"],
                "They have 7 days to accept."),
            "send_passkey_recovery": (["{escaped_link}", "{escaped_frontend}"],
                "This link expires in 15 minutes and can only be used once."),
            "send_account_recovery": (["{escaped_link}", "{escaped_frontend}"],
                "This link expires in 15 minutes and can only be used once."),
            "send_verification_email": (["{escaped_link}", "{escaped_frontend}"],
                "This link expires in 24 hours."),
        }
        for name, (links, expiry) in expected.items():
            with self.subTest(message=name):
                method = methods[name]
                self.assertEqual(re.findall(r'href="([^"]+)"', method), links)
                self.assertIn(expiry, method)
                self.assertIn("Some(&text_body)", method)
        for name in ("send_passkey_recovery", "send_account_recovery", "send_verification_email"):
            with self.subTest(escaping=name):
                self.assertIn("html_escape(display_name)", methods[name])
                self.assertRegex(methods[name], r'html_escape\((recovery|verification)_link\)')
        for field in ("email", "inviter_name", "workspace_name"):
            self.assertIn(f"html_escape({field})", methods["send_workspace_invitation"])
        for field in ("label", "value", "subject"):
            self.assertIn(f"html_escape({field})", methods["send_admin_notification"])


if __name__ == "__main__":
    unittest.main()
