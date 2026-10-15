"""Minimal process environment for finite CLI fixtures."""
import os


def fixture_environment(**declared: str) -> dict[str, str]:
    """Keep Windows loader/crypto bootstrap state, excluding ambient secrets."""
    environment = {"PATH": os.defpath}
    if "SystemRoot" in os.environ:
        environment["SystemRoot"] = os.environ["SystemRoot"]
    environment.update(declared)
    return environment
