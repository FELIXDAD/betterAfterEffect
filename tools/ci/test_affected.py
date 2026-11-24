"""Tests for affected.py: `python3 -m unittest discover -s tools/ci`."""

import unittest

from affected import classify, downstream, literal_references, member_globs

MEMBERS = {
    "crates/bae-model": "bae-model",
    "crates/bae-ui": "bae-ui",
    "crates/bae-ui-motion": "bae-ui-motion",
    "crates/bae-agent": "bae-agent",
    "apps/bae": "bae",
    "tools/dep-probe": "bae-dep-probe",
    "xtask": "xtask",
}
DEPS = {
    "bae-model": set(),
    "bae-ui": {"bae-model"},
    "bae-ui-motion": {"bae-ui", "bae-model"},
    "bae-agent": {"bae-model"},
    "bae": {"bae-ui-motion", "bae-agent"},
    "bae-dep-probe": set(),
    "xtask": set(),
}
GLOBS = (["apps", "crates", "services"], ["xtask", "tools/dep-probe"])
REFS = {
    "bae-ui-motion": {"docs/reference/ae/menus.json", "docs/ui/ae-commands.md"},
    "bae-agent": {"docs/agent-guide/mcp-instructions"},
    "xtask": {"tools/size-budget/budget.toml"},
}


def mode_of(*changed):
    mode, direct, _ = classify(list(changed), MEMBERS, *GLOBS, REFS)
    return mode, direct


class Classify(unittest.TestCase):
    def test_docs_nobody_reads_build_nothing(self):
        self.assertEqual(mode_of("docs/spec/ci.md", "README.md", "tools/ae/aectl"), ("none", set()))

    def test_docs_a_crate_reads_test_that_crate(self):
        self.assertEqual(mode_of("docs/ui/ae-commands.md"), ("crates", {"bae-ui-motion"}))
        self.assertEqual(mode_of("docs/agent-guide/mcp-instructions/en.md"), ("crates", {"bae-agent"}))
        self.assertEqual(mode_of("tools/size-budget/budget.toml"), ("crates", {"xtask"}))

    def test_a_crate_change_tests_the_crate(self):
        self.assertEqual(mode_of("crates/bae-ui/src/theme.rs"), ("crates", {"bae-ui"}))
        # Longest directory wins: bae-ui-motion is not bae-ui.
        self.assertEqual(mode_of("crates/bae-ui-motion/src/lib.rs"), ("crates", {"bae-ui-motion"}))
        self.assertEqual(mode_of("tools/dep-probe/src/lib.rs"), ("crates", {"bae-dep-probe"}))

    def test_workspace_inputs_mean_full(self):
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "rustfmt.toml",
            ".github/workflows/ci.yml",
            ".cargo/config.toml",
            "vendor/epaint/src/lib.rs",
            "xtask/src/size.rs",
        ]:
            self.assertEqual(mode_of(path, "crates/bae-ui/src/theme.rs")[0], "full", path)

    def test_a_crate_outside_the_members_means_full(self):
        # Added, removed or renamed crates change the workspace.
        self.assertEqual(mode_of("crates/bae-new/src/lib.rs")[0], "full")
        self.assertEqual(mode_of("services/bae-hub/Cargo.toml")[0], "full")


class Downstream(unittest.TestCase):
    def test_reverse_dependencies_are_transitive(self):
        self.assertEqual(downstream({"bae-ui"}, DEPS), {"bae-ui", "bae-ui-motion", "bae"})
        self.assertEqual(
            downstream({"bae-model"}, DEPS),
            {"bae-model", "bae-ui", "bae-ui-motion", "bae-agent", "bae"},
        )
        self.assertEqual(downstream({"bae"}, DEPS), {"bae"})


class Literals(unittest.TestCase):
    NAMES = ["docs", "tools", "assets", "README.md"]

    def test_relative_and_manifest_dir_paths(self):
        text = """
        const A: &str = include_str!("../../../../docs/reference/ae/menus.json");
        const B: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/ui/ae-commands.md");
        let c = root.join("assets/icons/");
        let d = include_str!("../../README.md");
        """
        self.assertEqual(
            literal_references(text, self.NAMES),
            {"docs/reference/ae/menus.json", "docs/ui/ae-commands.md", "assets/icons", "README.md"},
        )

    def test_bare_words_are_not_paths(self):
        text = 'json!({"tools": [], "docs": 1}); let x = "my-tools/a"; // see docs/spec/ci.md'
        self.assertEqual(literal_references(text, self.NAMES), set())


class Globs(unittest.TestCase):
    def test_member_patterns(self):
        manifest = '[workspace]\nmembers = ["apps/*", "crates/*", "xtask", "tools/dep-probe"]\n'
        self.assertEqual(member_globs(manifest), (["apps", "crates"], ["xtask", "tools/dep-probe"]))


if __name__ == "__main__":
    unittest.main()
