#!/usr/bin/env python3
"""Parse After Effects default keyboard shortcuts and preference text files
into clean, privacy-sanitized intermediate JSON files.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
from typing import Any


SKIP_PREF_SECTION_PREFIXES = (
    "AIRelight",
    "CEP_",
    "DeepGlow",
    "L3tt3rM4pp3r",
    "Modulation",
    "Settings_Felix",
    "Settings_Motion",
    "Settings_aescripts",
    "com.rimlight",
    "com.swarm",
    "Most Recently Used",
    "Auto Mount Server",
    "Coachmarks Seen By User",
    "Adobe PIP & CR Data Logging",
    "Expression Editor Theme (v9) - ",
)

SENSITIVE_KEY_PATTERNS = re.compile(
    r"(path|folder|dir|directory|file|mru|recent|user|account|email|token|license|"
    r"serial|uuid|guid|device|hardware|monitor|gpu|audio hardware|last project|"
    r"cache folder|database folder|template project|workspace)",
    re.IGNORECASE,
)

DRIVE_PATH_RE = re.compile(r"[A-Za-z]:[/\\]|\\\\[A-Za-z0-9_.-]+\\|/Users/|/home/")
GUID_RE = re.compile(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
USER_TOKEN_RE = re.compile(r"\b(king|felix)\b", re.IGNORECASE)


def decode_ae_pref_token(raw: str) -> tuple[str | None, bytes]:
    """Decode Adobe's mixed quoted-text and hex-byte preference encoding."""
    buf = bytearray()
    i = 0
    n = len(raw)
    while i < n:
        ch = raw[i]
        if ch == '"':
            i += 1
            while i < n and raw[i] != '"':
                buf.extend(raw[i].encode("utf-8"))
                i += 1
            if i < n and raw[i] == '"':
                i += 1
        elif ch.isspace():
            i += 1
        else:
            # Read contiguous hex chars
            j = i
            while j < n and raw[j] in "0123456789abcdefABCDEF":
                j += 1
            hex_chunk = raw[i:j]
            if len(hex_chunk) >= 2 and len(hex_chunk) % 2 == 0:
                buf.extend(bytes.fromhex(hex_chunk))
                i = j
            else:
                # Fallback literal char
                buf.extend(raw[i].encode("utf-8"))
                i += 1
    raw_bytes = bytes(buf)
    # Detect binary data (NUL or non-whitespace control chars)
    if any(b < 0x09 or (0x0E <= b < 0x20) or b == 0x7F for b in raw_bytes):
        return None, raw_bytes
    try:
        text = raw_bytes.decode("utf-8")
        return text, raw_bytes
    except UnicodeDecodeError:
        return None, raw_bytes


def parse_ae_ini_file(path: pathlib.Path) -> list[tuple[str, list[tuple[str, str]]]]:
    """Parse an AE text preference or shortcut file into [(section, [(key, raw_val)])]."""
    raw_lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    # Join backslash-continued lines
    merged: list[str] = []
    acc = ""
    for line in raw_lines:
        if acc:
            stripped = line.lstrip(" \t")
            if stripped.endswith("\\"):
                acc += stripped[:-1]
            else:
                acc += stripped
                merged.append(acc)
                acc = ""
        else:
            if line.endswith("\\"):
                acc = line[:-1]
            else:
                merged.append(line)
    if acc:
        merged.append(acc)

    sections: list[tuple[str, list[tuple[str, str]]]] = []
    cur_sec: str | None = None
    cur_items: list[tuple[str, str]] = []

    for line in merged:
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        if s.startswith("[") and s.endswith("]"):
            if cur_sec is not None:
                sections.append((cur_sec, cur_items))
            sec_text, _ = decode_ae_pref_token(s[1:-1].strip())
            cur_sec = sec_text if sec_text is not None else s[1:-1].strip().strip('"')
            cur_items = []
        elif "=" in s and cur_sec is not None:
            left, right = s.split("=", 1)
            key_text, _ = decode_ae_pref_token(left.strip())
            key = key_text if key_text is not None else left.strip().strip('"')
            cur_items.append((key, right.strip()))

    if cur_sec is not None:
        sections.append((cur_sec, cur_items))
    return sections


def parse_shortcuts(aeks_file: pathlib.Path) -> dict[str, Any]:
    sections = parse_ae_ini_file(aeks_file)
    contexts: dict[str, list[dict[str, Any]]] = {}
    tools: list[dict[str, Any]] = []
    total_commands = 0

    for sec_name, items in sections:
        if sec_name == "** header **":
            continue
        entries: list[dict[str, Any]] = []
        for cmd, raw_val in items:
            val_text, _ = decode_ae_pref_token(raw_val)
            val_str = val_text if val_text is not None else raw_val.strip('"')
            bindings = [m for m in re.findall(r"\(([^)]*)\)", val_str) if m != ""]
            entry = {
                "command": cmd,
                "shortcuts": bindings,
            }
            entries.append(entry)
            total_commands += 1
            if sec_name == "CEggAppTool":
                tools.append(entry)
        contexts[sec_name] = entries

    return {
        "contextCount": len(contexts),
        "commandCount": total_commands,
        "tools": tools,
        "contexts": contexts,
    }


def sanitize_pref_value(section: str, key: str, raw_val: str) -> Any:
    text, raw_bytes = decode_ae_pref_token(raw_val)
    if text is None:
        return f"<binary:{len(raw_bytes)}B>"
    stripped = text.strip()
    if stripped.startswith("<?xml") or "<AfterEffectsProject" in stripped:
        return f"<xml:{len(raw_bytes)}B>"
    if len(stripped) > 300:
        if DRIVE_PATH_RE.search(stripped) or GUID_RE.search(stripped) or USER_TOKEN_RE.search(stripped):
            return "<redacted:blob>"
        return f"<long_string:{len(stripped)}chars>"
    if DRIVE_PATH_RE.search(stripped):
        return "<redacted:path>"
    if GUID_RE.search(stripped):
        return "<redacted:guid>"
    if USER_TOKEN_RE.search(stripped):
        return "<redacted:user>"
    if SENSITIVE_KEY_PATTERNS.search(key) and len(stripped) > 4:
        # Preserve pure numeric/boolean settings even if key name has words like 'folder' or 'file'
        if not re.fullmatch(r"[-+]?\d+(\.\d+)?", stripped):
            return "<redacted:local>"
    return stripped


def parse_preferences(prefs_dir: pathlib.Path, version_prefix: str = "Adobe After Effects 26.0") -> dict[str, Any]:
    pref_files = sorted(
        p for p in prefs_dir.glob(f"{version_prefix}*.txt")
        if not p.name.endswith("-effects.txt")
    )
    sections_out: dict[str, dict[str, Any]] = {}
    skipped_sections: list[str] = []
    source_files: list[str] = []

    for pf in pref_files:
        # Categorize by suffix (without leaking localized filename verbatim if not needed)
        suffix = pf.stem.replace(version_prefix, "").strip()
        file_tag = "main"
        if "-indep-general" in suffix:
            file_tag = "indep-general"
        elif "-indep-composition" in suffix:
            file_tag = "indep-composition"
        elif "-indep-render" in suffix:
            file_tag = "indep-render"
        elif "-indep-output" in suffix:
            file_tag = "indep-output"
        elif "-paint" in suffix:
            file_tag = "paint"
        elif "-text" in suffix:
            file_tag = "text"
        source_files.append(file_tag)

        for sec_name, items in parse_ae_ini_file(pf):
            clean_sec = sec_name.strip()
            if any(clean_sec.startswith(pfx) for pfx in SKIP_PREF_SECTION_PREFIXES):
                if clean_sec not in skipped_sections:
                    skipped_sections.append(clean_sec)
                continue

            # For binary-heavy output/render preset sections, keep key schema compact
            if file_tag == "indep-output":
                sec_dict = sections_out.setdefault(clean_sec, {})
                sec_dict["_file"] = file_tag
                sec_dict["_binaryPresetEntryCount"] = len(items)
                continue

            sec_dict = sections_out.setdefault(clean_sec, {})
            if "_file" not in sec_dict:
                sec_dict["_file"] = file_tag
            for k, raw_v in items:
                if USER_TOKEN_RE.search(k):
                    continue
                sec_dict[k] = sanitize_pref_value(clean_sec, k, raw_v)

    total_keys = sum(
        sum(1 for k in v if not k.startswith("_"))
        for v in sections_out.values()
    )
    return {
        "fileGroups": source_files,
        "sectionCount": len(sections_out),
        "keyCount": total_keys,
        "excludedSectionCount": len(skipped_sections),
        "sections": sections_out,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Parse AE shortcuts and preferences into JSON.")
    parser.add_argument(
        "--prefs-dir",
        default=os.path.join(os.environ.get("APPDATA", ""), "Adobe", "After Effects", "26.0"),
        help="Path to After Effects version preferences directory",
    )
    parser.add_argument(
        "--out-dir",
        default=r"E:\bae-test\ref-ae\raw",
        help="Output directory for parsed JSON",
    )
    args = parser.parse_args()

    prefs_dir = pathlib.Path(args.prefs_dir)
    out_dir = pathlib.Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    # Locate default shortcuts file in aeks/
    aeks_dir = prefs_dir / "aeks"
    aeks_candidates = sorted(aeks_dir.glob("After Effects*.txt"))
    if not aeks_candidates:
        raise FileNotFoundError(f"No default shortcuts file found in {aeks_dir}")
    shortcuts_data = parse_shortcuts(aeks_candidates[0])
    (out_dir / "shortcuts_raw.json").write_text(
        json.dumps(shortcuts_data, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )

    prefs_data = parse_preferences(prefs_dir)
    (out_dir / "preferences_raw.json").write_text(
        json.dumps(prefs_data, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )

    print(
        f"Parsed shortcuts: {shortcuts_data['contextCount']} contexts, "
        f"{shortcuts_data['commandCount']} commands, {len(shortcuts_data['tools'])} tools"
    )
    print(
        f"Parsed preferences: {prefs_data['sectionCount']} sections, "
        f"{prefs_data['keyCount']} keys (excluded {prefs_data['excludedSectionCount']} third-party/MRU/theme sections)"
    )


if __name__ == "__main__":
    main()
