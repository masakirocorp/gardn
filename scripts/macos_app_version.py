#!/usr/bin/env python3

from __future__ import annotations

import re
import sys


_MAX_SIGNED_64 = (1 << 63) - 1
_VERSION_PATTERN = re.compile(
    r"^(?P<major>0|[1-9][0-9]*)\."
    r"(?P<minor>0|[1-9][0-9]*)\."
    r"(?P<patch>0|[1-9][0-9]*)"
    r"(?:-beta\.(?P<beta>0|[1-9][0-9]*))?$"
)


def bundle_version(version: str) -> str:
    """Return the deterministic numeric version embedded in a Gardn app bundle.

    Stable releases use stage 999; beta stages 0 through 998 sort before the
    corresponding stable release. Every emitted component must fit Sparkle's
    signed 64-bit ``longLongValue`` representation.
    """
    if not isinstance(version, str):
        raise ValueError(
            "version must be a string in canonical X.Y.Z or X.Y.Z-beta.N form"
        )

    match = _VERSION_PATTERN.fullmatch(version)
    if match is None:
        raise ValueError(
            f"invalid version {version!r}; expected canonical X.Y.Z or "
            "X.Y.Z-beta.N with no leading zeroes"
        )

    major = int(match.group("major"))
    minor = int(match.group("minor"))
    patch = int(match.group("patch"))
    beta = match.group("beta")
    stage = int(beta) if beta is not None else 999
    if beta is not None and stage > 998:
        raise ValueError(
            f"beta stage {stage} is out of range; use a beta stage from 0 through 998"
        )

    encoded = (10 + major, minor, patch * 1000 + stage)
    component_names = ("major", "minor", "patch/stage")
    for name, component in zip(component_names, encoded):
        if component > _MAX_SIGNED_64:
            raise ValueError(
                f"encoded {name} component {component} for version {version!r} "
                f"exceeds signed 64-bit maximum {_MAX_SIGNED_64}"
            )

    return ".".join(str(component) for component in encoded)


def main(argv: list[str] | None = None) -> int:
    arguments = sys.argv[1:] if argv is None else argv
    if len(arguments) != 1:
        print("usage: macos_app_version.py VERSION", file=sys.stderr)
        return 2

    try:
        encoded = bundle_version(arguments[0])
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    print(encoded)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
