#!/usr/bin/env python3
"""Check that extraction observations do not turn missing evidence into success."""
import importlib.util
import pathlib
import unittest
import urllib.error
from unittest.mock import patch

path = pathlib.Path(__file__).with_name("extraction-readiness.py")
spec = importlib.util.spec_from_file_location("readiness", path)
readiness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(readiness)


class ReadinessTests(unittest.TestCase):
    def test_dependencies_respect_repository_boundary(self):
        entries = [{"name": "first", "crates": ["a", "b"]}, {"name": "second", "crates": ["c"]}, {"name": "missing", "crates": ["d"]}]
        packages = [{"name": "a", "version": "1.0.0", "dependencies": [{"name": "b", "req": "^1"}, {"name": "c", "req": "^2"}]},
                    {"name": "b", "version": "1.0.0", "dependencies": []}, {"name": "c", "version": "2.0.0", "dependencies": []}]
        rows = readiness.build_rows(entries, {"packages": packages})
        self.assertEqual(rows[0]["crates"][0]["dependencies"], [{"name": "c", "requirement": "^2", "repository": "second"}])
        self.assertEqual(rows[2]["crates"][0]["local"], "not in workspace")
        self.assertEqual(rows[0]["verification"], "not checked")

    def test_registry_404_differs_from_server_failure(self):
        with patch.object(readiness.urllib.request, "urlopen", side_effect=urllib.error.HTTPError("url", 404, "missing", {}, None)):
            self.assertEqual(readiness.registry_version("name", "1.0.0")["state"], "absent")
        with patch.object(readiness.urllib.request, "urlopen", side_effect=urllib.error.HTTPError("url", 503, "down", {}, None)):
            self.assertEqual(readiness.registry_version("name", "1.0.0")["state"], "unknown")

    def test_registry_reports_yanked(self):
        from io import BytesIO
        with patch.object(readiness.urllib.request, "urlopen", return_value=BytesIO(b'{"version":{"yanked":true}}')):
            self.assertEqual(readiness.registry_version("name", "1.0.0"), {"state": "published", "yanked": True})


if __name__ == "__main__":
    unittest.main()
