"""Exercise the shared cache map with Nginx in an explicitly selected local image.

Run with OCTACITY_GATEWAY_TEST_IMAGE=octacity/gateway:local; no image is pulled,
no host port is published, and no running stand is changed.
"""

from __future__ import annotations

import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest

REPOSITORY = Path(__file__).resolve().parents[2]
CACHE_MAP = REPOSITORY / "deployment/console/nginx/cache-map.conf"
sys.path.insert(0, str(REPOSITORY / "tools"))
from local_stand_build_inputs import CONTAINER_PLATFORM


@unittest.skipUnless(
    os.environ.get("OCTACITY_GATEWAY_TEST_IMAGE"),
    "set OCTACITY_GATEWAY_TEST_IMAGE to test the cache map with a local Nginx image",
)
class ConsoleCacheRuntimeTests(unittest.TestCase):
    def test_cache_headers_cover_compact_and_legacy_hashes_without_caching_other_routes(self):
        immutable = "public, max-age=31536000, immutable"
        cases = [
            ("/assets/3U4pyH30.js", immutable),
            ("/assets/Ab12_-cD.js", immutable),
            ("/assets/q7w8e9r0.css", immutable),
            ("/assets/1a2B3c4D.woff2", immutable),
            ("/assets/ui-Ab12_-cD.js", immutable),
            ("/assets/vendor-AaBb1234xy.js", immutable),
            ("/assets/Ab12_-cD.js?v=2", immutable),
            ("/", "no-store"),
            ("/index.html", "no-store"),
            ("/builds/example", "no-store"),
            ("/api/v1/items", "no-store"),
            ("/health/ready", "no-store"),
            ("/assets/app.js", "no-store"),
            ("/assets/abcdefg.js", "no-store"),
            ("/assets/abcdefghi.js", "no-store"),
            ("/static/Ab12_-cD.js", "no-store"),
            ("/assets/nested/Ab12_-cD.js", "no-store"),
            ("/assets/Ab12_-cD.js.map", "no-store"),
        ]
        configuration = """pid /tmp/cache-test.pid;
error_log /dev/stderr notice;
events {}
http {
    access_log off;
    client_body_temp_path /tmp/client;
    proxy_temp_path /tmp/proxy;
    fastcgi_temp_path /tmp/fastcgi;
    uwsgi_temp_path /tmp/uwsgi;
    scgi_temp_path /tmp/scgi;
    include /tmp/cache-map.conf;
    server {
        listen 8080;
        add_header Cache-Control $octacity_console_cache_control always;
        location / { return 200 "fixture"; }
    }
}
"""
        checks = [
            "nginx -c /tmp/cache-test.conf",
            "trap 'nginx -c /tmp/cache-test.conf -s quit >/dev/null 2>&1 || true' EXIT",
        ]
        for path, expected in cases:
            checks.extend([
                f"echo {shlex.quote('checking cache header: ' + path)}",
                "curl --fail --silent --show-error --dump-header /tmp/headers "
                f"--output /dev/null {shlex.quote('http://127.0.0.1:8080' + path)}",
                "tr -d '\\r' </tmp/headers | grep -Fxi -- "
                + shlex.quote("Cache-Control: " + expected),
            ])
        with tempfile.TemporaryDirectory(prefix="oc-console-cache-") as temporary:
            config = Path(temporary) / "nginx.conf"
            config.write_text(configuration, encoding="utf-8")
            config.chmod(0o444)
            result = subprocess.run(
                [
                    "docker", "run", "--rm", "--pull=never",
                    "--platform", CONTAINER_PLATFORM,
                    "--network", "none", "--read-only",
                    "--tmpfs", "/tmp:rw,noexec,nosuid,nodev,size=16m",
                    "--mount",
                    f"type=bind,src={config},dst=/tmp/cache-test.conf,readonly",
                    "--mount",
                    f"type=bind,src={CACHE_MAP},dst=/tmp/cache-map.conf,readonly",
                    "--entrypoint", "/bin/sh",
                    os.environ["OCTACITY_GATEWAY_TEST_IMAGE"],
                    "-ec", "\n".join(checks),
                ],
                capture_output=True,
                text=True,
                timeout=30,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
