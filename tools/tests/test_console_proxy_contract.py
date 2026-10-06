"""Contract tests for the supported same-origin console proxy fixture."""

from pathlib import Path
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
CONSOLE_NGINX = REPOSITORY / "deployment/console/nginx"
PRODUCTION = CONSOLE_NGINX / "nginx.conf"
ROUTES = CONSOLE_NGINX / "routes.conf"
SECURITY_HEADERS = CONSOLE_NGINX / "security-headers.conf"
CACHE_MAP = CONSOLE_NGINX / "cache-map.conf"


class ConsoleProxyContractTests(unittest.TestCase):
    def test_production_fixture_terminates_tls_and_keeps_management_private(self):
        config = PRODUCTION.read_text(encoding="utf-8")

        self.assertIn("listen 443 ssl;", config)
        self.assertIn("ssl_protocols TLSv1.2 TLSv1.3;", config)
        self.assertIn("server 127.0.0.1:8080;", config)
        self.assertNotIn("proxy_pass http://127.0.0.1:8080", config)
        self.assertIn("root /srv/octacity-console/current;", config)
        self.assertIn("include /etc/nginx/mime.types;", config)
        self.assertIn("default_type application/octet-stream;", config)

    def test_console_discards_browser_credentials_and_upstream_policy(self):
        headers = SECURITY_HEADERS.read_text(encoding="utf-8")

        for identity_header in (
            'proxy_set_header Authorization "";',
            'proxy_set_header X-Forwarded-User "";',
            'proxy_set_header X-Remote-User "";',
        ):
            self.assertIn(identity_header, headers)
        for response_header in (
            "Access-Control-Allow-Credentials",
            "Access-Control-Allow-Origin",
            "Cache-Control",
            "Content-Security-Policy",
            "Expires",
            "Referrer-Policy",
            "Strict-Transport-Security",
        ):
            self.assertIn(f"proxy_hide_header {response_header};", headers)

        local = (REPOSITORY / "deployment/local-stand/nginx.conf").read_text(
            encoding="utf-8"
        )
        self.assertNotIn('proxy_set_header Authorization "";', local)

    def test_api_and_health_routes_cannot_fall_through_to_the_spa(self):
        routes = ROUTES.read_text(encoding="utf-8")

        for prefix in ("/api/v1", "/health"):
            self.assertIn(f"location = {prefix}", routes)
            self.assertIn(f"location ^~ {prefix}/", routes)
        self.assertEqual(routes.count("proxy_pass http://octacity_management;"), 4)
        api_end = routes.index("# Static files")
        self.assertNotIn("index.html", routes[:api_end])

    def test_only_non_file_ui_routes_receive_the_spa_fallback(self):
        routes = ROUTES.read_text(encoding="utf-8")

        self.assertIn("location ^~ /assets/", routes)
        self.assertIn("location ~ \\.[^/]+$", routes)
        self.assertGreaterEqual(routes.count("try_files $uri =404;"), 3)
        self.assertEqual(routes.count("try_files $uri $uri/ /index.html;"), 1)

    def test_cache_policy_is_immutable_only_for_content_hashed_assets(self):
        cache = CACHE_MAP.read_text(encoding="utf-8")

        self.assertIn('default "no-store";', cache)
        self.assertIn("^/assets/", cache)
        self.assertIn("{8,}", cache)
        self.assertIn("max-age=31536000, immutable", cache)

    def test_security_policy_restricts_code_network_framing_and_referrers(self):
        headers = SECURITY_HEADERS.read_text(encoding="utf-8")

        for directive in (
            "default-src 'none'",
            "connect-src 'self'",
            "frame-ancestors 'none'",
            "script-src 'self'",
            "script-src-attr 'none'",
            "style-src 'self'",
            "X-Content-Type-Options \"nosniff\"",
            "X-Frame-Options \"DENY\"",
            "Referrer-Policy \"no-referrer\"",
            "Strict-Transport-Security \"max-age=31536000\"",
        ):
            self.assertIn(directive, headers)

    def test_local_stand_and_production_use_the_same_contract_snippets(self):
        production = PRODUCTION.read_text(encoding="utf-8")
        local = (REPOSITORY / "deployment/local-stand/nginx.conf").read_text(
            encoding="utf-8"
        )

        for include in (
            "include /etc/nginx/octacity-console/cache-map.conf;",
            "include /etc/nginx/octacity-console/security-headers.conf;",
            "include /etc/nginx/octacity-console/routes.conf;",
        ):
            self.assertIn(include, production)
            self.assertIn(include, local)

    def test_runbook_uses_the_packaged_proxy_and_atomic_static_switch(self):
        guide = (REPOSITORY / "docs/operations/operator-console.md").read_text(
            encoding="utf-8"
        )

        self.assertIn('NGINX_SOURCE="$PWD/share/nginx"', guide)
        self.assertIn("/srv/octacity-console/releases/$RELEASE_ID", guide)
        self.assertIn(
            "sudo mv -Tf /srv/octacity-console/.current-next "
            "/srv/octacity-console/current",
            guide,
        )
        self.assertNotIn("cp -R ui/dist/. /srv/octacity-console/", guide)

    def test_operator_documentation_keeps_the_no_login_boundary_explicit(self):
        management = (
            REPOSITORY / "docs/reference/management-rest-v1.md"
        ).read_text(encoding="utf-8")
        architecture = (REPOSITORY / "docs/architecture/server.md").read_text(
            encoding="utf-8"
        )
        threat_model = (
            REPOSITORY / "docs/architecture/threat-model.md"
        ).read_text(encoding="utf-8")
        guide = (REPOSITORY / "docs/operations/operator-console.md").read_text(
            encoding="utf-8"
        )

        self.assertIn("There is no login", management)
        self.assertIn("no authenticated browser identity", architecture)
        self.assertIn("Deploying the console does not add login", threat_model)
        self.assertIn("Access to this origin is management authority", guide)
        self.assertIn("The console remains optional", " ".join(guide.split()))


if __name__ == "__main__":
    unittest.main()
