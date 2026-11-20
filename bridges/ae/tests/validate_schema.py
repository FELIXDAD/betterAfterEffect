#!/usr/bin/env python3
"""Validate BAE interchange / anim-sample files against their JSON Schemas.

Usage:
    python3 bridges/ae/tests/validate_schema.py FILE.json [FILE.json ...]

The schema is chosen from the file's "format" field. Needs the `jsonschema` package
(pip install jsonschema); exits with status 2 when it is missing.
"""
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
SCHEMAS = {
    "bae-interchange": ROOT / "docs/interop/bae-interchange.schema.json",
    "bae-anim-cases": ROOT / "docs/interop/ae-anim-samples.schema.json",
    "bae-anim-samples": ROOT / "docs/interop/ae-anim-samples.schema.json",
}


def registry():
    """All schemas by $id, so ae-anim-samples can $ref the interchange definitions."""
    from referencing import Registry, Resource

    resources = []
    for path in set(SCHEMAS.values()):
        contents = json.loads(path.read_text(encoding="utf-8"))
        resources.append((contents["$id"], Resource.from_contents(contents)))
    return Registry().with_resources(resources)


def main(paths):
    try:
        import jsonschema
    except ImportError:
        print("jsonschema is not installed: pip install jsonschema", file=sys.stderr)
        return 2
    failed = 0
    for p in paths:
        doc = json.loads(pathlib.Path(p).read_text(encoding="utf-8"))
        schema_path = SCHEMAS.get(doc.get("format"))
        if schema_path is None:
            print(f"{p}: unknown format {doc.get('format')!r}")
            failed += 1
            continue
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        cls = jsonschema.validators.validator_for(schema)
        cls.check_schema(schema)
        validator = cls(schema, registry=registry())
        errors = sorted(validator.iter_errors(doc), key=lambda e: list(e.absolute_path))
        for e in errors[:20]:
            loc = "/".join(str(x) for x in e.absolute_path)
            print(f"{p}: {loc}: {e.message[:300]}")
        if errors:
            print(f"{p}: {len(errors)} error(s)")
            failed += 1
        else:
            print(f"{p}: OK ({schema_path.name})")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
