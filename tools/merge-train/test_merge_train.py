#!/usr/bin/env python3
"""Offline tests for merge_train.py: `python3 tools/merge-train/test_merge_train.py`.

GitHub and Orca are mocked; overlap and Cargo.lock repair run against a throwaway git
repository with a local bare "origin".
"""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import merge_train as mt  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(HERE)), "tools", "ci"))
import affected  # noqa: E402  (tools/ci/affected.py of this checkout)
sys.path.pop(0)

# A small fake workspace: fx depends on render; ui and other stand alone. docs/ui/menu.md
# is read by ui's sources (string literal), so it counts as a ui change.
FAKE_GRAPH = (affected,
              {"crates/fx": "fx", "crates/render": "render", "crates/ui": "ui", "crates/other": "other"},
              {"fx": {"render"}, "render": set(), "ui": set(), "other": set()},
              ["crates"], [], {"ui": {"docs/ui/menu.md"}})


def run(cwd, *cmd):
    return subprocess.run(cmd, cwd=cwd, check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, universal_newlines=True).stdout.strip()


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="mt-test-")
        mt.CFG = mt.load_toml(os.path.join(HERE, "config.toml"))
        mt.LEADERS = mt.load_toml(os.path.join(HERE, "leaders.toml"))
        mt.CFG["state_dir"] = os.path.join(self.tmp, "state")
        mt.CFG["work_dir"] = os.path.join(self.tmp, "work")
        mt.CFG["cargo_target_dir"] = os.path.join(self.tmp, "target")
        mt.CFG["runner_dir"] = os.path.join(self.tmp, "repo")
        mt.ARGS = mt.argparse.Namespace(dry_run=False, once=True, simulate=[], ignore_draft=False)
        self._graph = mock.patch.object(mt, "crate_graph", return_value=FAKE_GRAPH)
        self._graph.start()

    def tearDown(self):
        self._graph.stop()
        shutil.rmtree(self.tmp, ignore_errors=True)

    def make_repo(self):
        """origin (bare) + clone at runner_dir with dev; returns clone path."""
        origin = os.path.join(self.tmp, "origin.git")
        repo = mt.CFG["runner_dir"]
        run(self.tmp, "git", "init", "-q", "--bare", "-b", "dev", origin)
        run(self.tmp, "git", "clone", "-q", origin, repo)
        for k, v in (("user.name", "t"), ("user.email", "t@example.invalid"),
                     ("commit.gpgsign", "false")):
            run(repo, "git", "config", k, v)
        self.write(repo, "a.txt", "a\n")
        self.write(repo, "docs/x.md", "x\n")
        self.write(repo, "Cargo.lock", "lock-base\n")
        run(repo, "git", "add", "-A")
        run(repo, "git", "commit", "-qm", "base")
        run(repo, "git", "push", "-q", "origin", "HEAD:dev")
        return repo

    @staticmethod
    def write(repo, path, text):
        p = os.path.join(repo, path)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            f.write(text)

    def commit(self, repo, files, msg):
        for path, text in files.items():
            self.write(repo, path, text)
        run(repo, "git", "add", "-A")
        run(repo, "git", "commit", "-qm", msg)
        return run(repo, "git", "rev-parse", "HEAD")


class TomlTest(unittest.TestCase):
    def test_subset(self):
        d = mt.toml_loads('a = "x # not comment"  # c\nb = [\n "p", # c\n "q",\n]\n'
                          "[t.u]\nn = 3\nok = true\n[[r]]\nk = 'lit'\n[[r]]\nk = \"\\u00e9\"\n")
        self.assertEqual(d["a"], "x # not comment")
        self.assertEqual(d["b"], ["p", "q"])
        self.assertEqual(d["t"]["u"], {"n": 3, "ok": True})
        self.assertEqual([r["k"] for r in d["r"]], ["lit", "é"])

    def test_repo_configs_parse(self):
        cfg = mt.load_toml(os.path.join(HERE, "config.toml"))
        self.assertTrue(cfg["checks"]["full"])
        leaders = mt.load_toml(os.path.join(HERE, "leaders.toml"))
        self.assertIn(leaders["default"], leaders["leaders"])
        for rule in leaders.get("rules", []):
            self.assertIn(rule["leader"], leaders["leaders"])


class OverlapTest(Base):
    def test_empty_on_both_sides_is_empty(self):
        repo = self.make_repo()
        head = run(repo, "git", "rev-parse", "HEAD")  # PR with no changes, dev unchanged
        self.assertEqual(mt.overlap(head), [])

    def test_overlap_and_ignores(self):
        repo = self.make_repo()
        run(repo, "git", "checkout", "-qb", "feat")
        pr = self.commit(repo, {"crates/ui/a.rs": "pr\n", "docs/x.md": "pr\n", "Cargo.lock": "pr\n"}, "pr")
        run(repo, "git", "checkout", "-q", "dev")
        self.commit(repo, {"docs/x.md": "dev\n", "Cargo.lock": "dev\n", "crates/other/b.rs": "b\n"}, "dev1")
        run(repo, "git", "push", "-q", "origin", "dev")
        run(repo, "git", "fetch", "-q", "origin")
        self.assertEqual(mt.overlap(pr), [])  # docs/ and Cargo.lock never count
        self.commit(repo, {"crates/ui/a.rs": "dev\n"}, "dev2")
        run(repo, "git", "push", "-q", "origin", "dev")
        run(repo, "git", "fetch", "-q", "origin")
        self.assertEqual(mt.overlap(pr), ["crates/ui/a.rs"])


class RunnerTest(Base):
    def test_runner_follows_ref_only_forward(self):
        repo = self.make_repo()
        mt.HERE = os.path.join(repo, "tools", "merge-train")  # pretend we run from the runner
        try:
            run(repo, "git", "checkout", "-qb", "trial")
            trial = self.commit(repo, {"tools/merge-train/merge_train.py": "v2\n"}, "trial")
            run(repo, "git", "checkout", "-q", "dev")
            self.commit(repo, {"tools/merge-train/merge_train.py": "v1\n"}, "dev has v1")
            run(repo, "git", "push", "-q", "origin", "dev")
            run(repo, "git", "checkout", "-q", "--detach", trial)
            self.assertFalse(mt.refresh_runner())  # dev does not contain the trial commit
            self.assertEqual(run(repo, "git", "rev-parse", "HEAD"), trial)
            run(repo, "git", "checkout", "-q", "dev")
            run(repo, "git", "merge", "-q", "--no-edit", "-X", "theirs", trial)
            run(repo, "git", "push", "-q", "origin", "dev")
            run(repo, "git", "checkout", "-q", "--detach", trial)
            mt.refresh_runner()  # now merged: moves forward to origin/dev
            self.assertEqual(run(repo, "git", "rev-parse", "HEAD"), run(repo, "git", "rev-parse", "origin/dev"))
        finally:
            mt.HERE = HERE


def pr(**kw):
    d = dict(number=7, title="t", url="u", isDraft=False, state="OPEN", baseRefName="dev",
             headRefName="core/x", headRefOid="h1", mergeable="MERGEABLE",
             isCrossRepository=False, labels=[], queued_at="2026-11-23T00:00:00Z")
    d.update(kw)
    return d


class DecideTest(Base):
    def decide(self, p, checks=("green", []), over=()):
        with mock.patch.object(mt, "evaluate_checks", return_value=checks), \
                mock.patch.object(mt, "fetch_pr_head"), \
                mock.patch.object(mt, "overlap", return_value=list(over)):
            return mt.decide(p, {})[0]

    def test_paths(self):
        self.assertEqual(self.decide(pr()), "merge")
        self.assertEqual(self.decide(pr(isDraft=True)), "reject")
        self.assertEqual(self.decide(pr(baseRefName="main")), "reject")
        self.assertEqual(self.decide(pr(mergeable="CONFLICTING")), "conflict")
        self.assertEqual(self.decide(pr(mergeable="UNKNOWN")), "wait-mergeable")
        self.assertEqual(self.decide(pr(), over=["a.rs"]), "update")
        self.assertEqual(self.decide(pr(), checks=("pending", [("x", "missing", "")])), "wait-ci")
        self.assertEqual(self.decide(pr(), checks=("failed", [("x", "failure", "")])), "ci-failed")

    def test_evaluate_checks(self):
        def runs(**states):
            out, i = {}, 0
            for name, (status, concl) in states.items():
                i += 1
                out[name] = {"id": i, "name": name, "status": status, "conclusion": concl,
                             "html_url": "url"}
            return out
        names = mt.required_checks()
        ok = {n: ("completed", "success") for n in names}
        with mock.patch.object(mt, "check_runs", return_value=runs(**ok)):
            self.assertEqual(mt.evaluate_checks("s")[0], "green")
        for bad, want in ((("completed", "skipped"), "pending"), (("in_progress", None), "pending"),
                          (("completed", "cancelled"), "pending"), (("completed", "failure"), "failed")):
            st = dict(ok)
            st[names[-1]] = bad
            with mock.patch.object(mt, "check_runs", return_value=runs(**st)):
                self.assertEqual(mt.evaluate_checks("s")[0], want, bad)
        st = dict(ok)
        del st[names[0]]
        with mock.patch.object(mt, "check_runs", return_value=runs(**st)):
            self.assertEqual(mt.evaluate_checks("s")[0], "pending")
        st = dict(ok, **{"other": ("completed", "failure")})
        with mock.patch.object(mt, "check_runs", return_value=runs(**st)):
            self.assertEqual(mt.evaluate_checks("s")[0], "failed")

    def test_newest_run_per_name_wins(self):
        api = {"total_count": 2, "check_runs": [
            {"id": 1, "name": "n", "status": "completed", "conclusion": "skipped"},
            {"id": 2, "name": "n", "status": "completed", "conclusion": "success"}]}
        with mock.patch.object(mt, "gh_json", return_value=api):
            self.assertEqual(mt.check_runs("s")["n"]["conclusion"], "success")


class ProcessTest(Base):
    def test_merge_uses_squash_for_ref_and_notifies_leader(self):
        # The repo turns per-merge messages off; this test covers the message itself.
        patcher = mock.patch.dict(mt.CFG, {"notify": {"merged": True}})
        patcher.start()
        self.addCleanup(patcher.stop)
        calls = []

        def fake_sh(cmd, **kw):
            calls.append(cmd)
            return (0, "", "") if kw.get("check") is False else ""
        p = pr(headRefName="ref/ae-x")
        with mock.patch.object(mt, "decide", return_value=("merge", "ok", {})), \
                mock.patch.object(mt, "sh", side_effect=fake_sh), \
                mock.patch.object(mt, "git", return_value="abc1234\n"), \
                mock.patch.object(mt, "orca_worktrees", return_value=[]), \
                mock.patch.object(mt, "send") as send:
            st = {}
            self.assertTrue(mt.process_head(p, st))
        merge = [c for c in calls if c[:3] == ["gh", "pr", "merge"]][0]
        self.assertIn("--squash", merge)
        self.assertIn("--match-head-commit", merge)
        self.assertEqual(send.call_args[0][0], mt.LEADERS["default"])
        self.assertIn("#7", send.call_args[0][1])
        self.assertEqual(st["merged"][-1]["dev"], "abc1234")

    def test_code_pr_uses_merge_commit(self):
        self.assertFalse(mt.squash_wanted(pr()))
        self.assertTrue(mt.squash_wanted(pr(labels=["reference-data"])))

    def test_ci_failure_drops_label_and_notifies(self):
        # The repo sends bounces only to the owner when one is found; this test covers both.
        patcher = mock.patch.dict(mt.CFG, {"notify": {"bounce_leader": True}})
        patcher.start()
        self.addCleanup(patcher.stop)
        wt = [{"host": "local", "name": "bae-03", "branch": "core/x", "comment": "", "status": "",
               "last_activity": 0, "agents": [{"orcaSessionId": "aaaa", "address": "orca_session_id:aaaa",
                                               "updatedAt": 1}]}]
        with mock.patch.object(mt, "decide", return_value=(
                "ci-failed", "CI 没过", {"checks": [("full / Windows x64", "failure", "http://x")]})), \
                mock.patch.object(mt, "orca_worktrees", return_value=wt), \
                mock.patch.object(mt, "remove_label") as rl, \
                mock.patch.object(mt, "comment") as cm, \
                mock.patch.object(mt, "send") as send:
            mt.process_head(pr(), {})
        rl.assert_called_once_with(7, "merge-ready")
        self.assertIn("full / Windows x64：failure http://x", cm.call_args[0][1])
        self.assertEqual([c[0][0] for c in send.call_args_list], ["orca_session_id:aaaa", mt.LEADERS["default"]])

    def test_wait_mergeable_unknown_is_not_failure(self):
        with mock.patch.object(mt, "decide", return_value=("wait-mergeable", "UNKNOWN", {})), \
                mock.patch.object(mt, "fail_pr") as fail:
            self.assertFalse(mt.process_head(pr(mergeable="UNKNOWN"), {}))
        fail.assert_not_called()

    def test_update_branch_then_stale_head_waits(self):
        calls = []

        def fake_sh(cmd, **kw):
            calls.append(cmd)
            return (0, "", "")
        st = {}
        with mock.patch.object(mt, "decide", return_value=("update", "overlap a.rs", {})), \
                mock.patch.object(mt, "sh", side_effect=fake_sh), \
                mock.patch.object(mt, "try_conflict_repair") as repair:
            self.assertFalse(mt.process_head(pr(), st))  # runs update-branch
            self.assertFalse(mt.process_head(pr(), st))  # GitHub still shows the old head
        self.assertEqual(sum(1 for c in calls if c[:3] == ["gh", "pr", "update-branch"]), 1)
        repair.assert_not_called()

    def test_dry_run_mutates_nothing(self):
        mt.ARGS.dry_run = True
        with mock.patch.object(mt, "decide", return_value=("merge", "ok", {})), \
                mock.patch.object(mt, "sh") as sh:
            mt.process_head(pr(), {})
        sh.assert_not_called()

    def test_leader_rules(self):
        milan = {"host": "米兰Mac", "name": "bae-13-ui-design"}
        local12 = {"host": "local", "name": "bae-12-menus"}
        # One leader since 2026-11-24: no routing rules, everything goes to the default.
        lead = mt.LEADERS["default"]
        self.assertEqual(mt.leader_for(pr(headRefName="ui/design"), milan), lead)
        self.assertEqual(mt.leader_for(pr(headRefName="fix/226"), local12), lead)
        self.assertEqual(mt.leader_for(pr(headRefName="ui/x"), None), lead)
        self.assertEqual(mt.leader_for(pr(headRefName="core/x"), None), lead)


class ConflictRepairTest(Base):
    def test_cargo_lock_only_conflict_is_regenerated_and_pushed(self):
        repo = self.make_repo()
        run(repo, "git", "checkout", "-qb", "feat")
        head = self.commit(repo, {"Cargo.lock": "lock-pr\n", "a.txt": "pr\n"}, "pr")
        run(repo, "git", "push", "-q", "origin", "feat")
        run(repo, "git", "checkout", "-q", "dev")
        self.commit(repo, {"Cargo.lock": "lock-dev\n", "b.txt": "b\n"}, "dev")
        run(repo, "git", "push", "-q", "origin", "dev")
        real_sh = mt.sh

        def fake_sh(cmd, **kw):
            if cmd[:2] == ["cargo", "metadata"]:  # stand-in for cargo re-resolving the lock
                with open(os.path.join(kw["cwd"], "Cargo.lock"), "a") as f:
                    f.write("regenerated\n")
                return (0, "{}", "")
            return real_sh(cmd, **kw)
        with mock.patch.object(mt, "sh", side_effect=fake_sh), \
                mock.patch.object(mt, "fetch_pr_head", side_effect=lambda n: run(repo, "git", "fetch", "-q", "origin")):
            ok, msg = mt.try_conflict_repair(pr(headRefName="feat", headRefOid=head))
        self.assertTrue(ok, msg)
        run(repo, "git", "fetch", "-q", "origin")
        self.assertEqual(run(repo, "git", "show", "origin/feat:Cargo.lock"), "lock-dev\nregenerated")
        parents = run(repo, "git", "log", "-1", "--format=%P", "origin/feat").split()
        self.assertEqual(len(parents), 2)

    def test_stale_scratch_dir_is_recreated(self):
        repo = self.make_repo()
        os.makedirs(os.path.join(mt.CFG["work_dir"], "junk"))  # e.g. left from an old layout
        run(repo, "git", "checkout", "-qb", "feat")
        head = self.commit(repo, {"c.txt": "pr\n"}, "pr")
        run(repo, "git", "push", "-q", "origin", "feat")
        with mock.patch.object(mt, "fetch_pr_head", side_effect=lambda n: run(repo, "git", "fetch", "-q", "origin")):
            ok, msg = mt.try_conflict_repair(pr(headRefName="feat", headRefOid=head))
        self.assertTrue(ok, msg)
        self.assertIn("无需推送", msg)  # dev had nothing new: no empty commit, no push
        self.assertFalse(os.path.exists(os.path.join(mt.CFG["work_dir"], "junk")))

    def test_real_conflict_is_reported(self):
        repo = self.make_repo()
        run(repo, "git", "checkout", "-qb", "feat")
        head = self.commit(repo, {"a.txt": "pr\n"}, "pr")
        run(repo, "git", "checkout", "-q", "dev")
        self.commit(repo, {"a.txt": "dev\n"}, "dev")
        run(repo, "git", "push", "-q", "origin", "dev")
        run(repo, "git", "push", "-q", "origin", "feat")
        with mock.patch.object(mt, "fetch_pr_head", side_effect=lambda n: run(repo, "git", "fetch", "-q", "origin")):
            ok, msg = mt.try_conflict_repair(pr(headRefName="feat", headRefOid=head))
        self.assertFalse(ok)
        self.assertIn("a.txt", msg)


class UnstickTest(Base):
    MISSING = ("pending", [("fast / macOS arm64", "missing", ""), ("full / macOS arm64", "skipped", "")])
    RUNNING = ("pending", [("fast / macOS arm64", "in_progress", ""), ("full / macOS arm64", "missing", "")])

    def wait(self, p, st, checks, minutes_stuck):
        if minutes_stuck is not None:
            ps = mt.pr_state(st, p["number"])
            ps["head"] = p["headRefOid"]
            ps["since"] = mt.now_utc().isoformat()
            ps["stuck_since"] = (mt.now_utc() - mt.dt.timedelta(minutes=minutes_stuck)).isoformat()
        with mock.patch.object(mt, "decide", return_value=("wait-ci", "等 CI", {"checks": checks[1]})), \
                mock.patch.object(mt, "nudge_ci") as nudge:
            mt.process_head(p, st)
        return nudge

    def test_missing_checks_nudge_once_after_delay(self):
        st = {}
        self.wait(pr(), st, self.MISSING, 3).assert_not_called()      # too early
        with mock.patch.object(mt, "gh_json", return_value=[{"workflow_runs": []}]):
            self.wait(pr(), st, self.MISSING, 11).assert_called_once()  # no fast run
        st["prs"]["7"]["nudged_head"] = "h1"
        self.wait(pr(), st, self.MISSING, 30).assert_not_called()     # once per head
        self.wait(pr(), {}, self.RUNNING, 60).assert_not_called()     # CI is running: wait

    def test_274_fast_workflow_precedes_required_windows_job(self):
        head = "67ca78203d05a0c638284313635554da28ba3cab"
        p = pr(number=274, headRefOid=head, headRefName="media/windows-mf")
        missing = ("pending", [("fast / Windows x64", "missing", "")])
        endpoint = "repos/%s/actions/workflows/ci-fast.yml/runs?head_sha=%s&per_page=100" % (
            mt.CFG["repo"], head)
        for status in ("queued", "in_progress", "waiting", "pending", "requested", "completed"):
            with self.subTest(status=status):
                st = {}
                pages = [{"workflow_runs": [{"status": status}]}]
                with mock.patch.object(mt, "gh_json", return_value=pages) as api:
                    nudge = self.wait(p, st, missing, 12)
                if status == "completed":
                    nudge.assert_called_once()
                else:
                    nudge.assert_not_called()
                    self.assertNotIn("stuck_since", mt.pr_state(st, 274))
                api.assert_called_once_with(["api", "--paginate", "--slurp", endpoint])

    def test_nudge_behind_runs_update_branch(self):
        repo = self.make_repo()
        run(repo, "git", "checkout", "-qb", "feat")
        head = self.commit(repo, {"c.txt": "pr\n"}, "pr")
        run(repo, "git", "push", "-q", "origin", "feat")
        run(repo, "git", "checkout", "-q", "dev")
        self.commit(repo, {"d.txt": "dev\n"}, "dev moved")
        run(repo, "git", "push", "-q", "origin", "dev")
        calls = []
        real_sh = mt.sh

        def fake_sh(cmd, **kw):
            if cmd[:2] == ["gh", "pr"]:
                calls.append(cmd)
                return (0, "", "")
            return real_sh(cmd, **kw)
        ps = {}
        with mock.patch.object(mt, "sh", side_effect=fake_sh), \
                mock.patch.object(mt, "fetch_pr_head", side_effect=lambda n: run(repo, "git", "fetch", "-q", "origin")):
            mt.nudge_ci(pr(headRefName="feat", headRefOid=head), ps)
        self.assertEqual(calls[0][:3], ["gh", "pr", "update-branch"])
        self.assertEqual(ps["nudged_head"], head)

    def test_nudge_up_to_date_pushes_empty_commit(self):
        repo = self.make_repo()
        run(repo, "git", "checkout", "-qb", "feat")
        head = self.commit(repo, {"c.txt": "pr\n"}, "pr")
        run(repo, "git", "push", "-q", "origin", "feat")
        with mock.patch.object(mt, "fetch_pr_head", side_effect=lambda n: run(repo, "git", "fetch", "-q", "origin")):
            mt.nudge_ci(pr(headRefName="feat", headRefOid=head), {})
        run(repo, "git", "fetch", "-q", "origin")
        self.assertEqual(run(repo, "git", "rev-parse", "origin/feat^"), head)
        self.assertEqual(run(repo, "git", "diff", "--name-only", head, "origin/feat"), "")

    def queue(self):
        return [pr(number=1, headRefName="a"), pr(number=2, headRefName="b", headRefOid="h2"),
                pr(number=3, headRefName="c", headRefOid="h3")]

    def test_skip_ahead_merges_first_ready_later_pr_after_30_min(self):
        verdicts = {1: ("wait-ci", "x", {}), 2: ("wait-ci", "y", {}), 3: ("merge", "ok", {})}
        st = {"head_pr": 1, "head_since": (mt.now_utc() - mt.dt.timedelta(minutes=31)).isoformat()}
        with mock.patch.object(mt, "decide", side_effect=lambda p, s: verdicts[p["number"]]), \
                mock.patch.object(mt, "fetch_pr_head"), \
                mock.patch.object(mt, "changed_files", side_effect=lambda sha: {
                    {"h1": "crates/ui/a.rs", "h2": "crates/other/b.rs", "h3": "crates/fx/c.rs"}[sha]}), \
                mock.patch.object(mt, "do_merge", return_value=True) as merge:
            self.assertTrue(mt.skip_ahead(self.queue(), st))
        self.assertEqual(merge.call_args[0][0]["number"], 3)

    def test_no_skip_ahead_before_30_min_or_without_ready_pr(self):
        st = {"head_pr": 1, "head_since": (mt.now_utc() - mt.dt.timedelta(minutes=10)).isoformat()}
        with mock.patch.object(mt, "decide", return_value=("merge", "ok", {})), \
                mock.patch.object(mt, "do_merge") as merge:
            self.assertFalse(mt.skip_ahead(self.queue(), st))
        merge.assert_not_called()
        st["head_since"] = (mt.now_utc() - mt.dt.timedelta(minutes=45)).isoformat()
        with mock.patch.object(mt, "decide", return_value=("update", "overlap", {})), \
                mock.patch.object(mt, "do_merge") as merge, \
                mock.patch.object(mt, "sh") as sh:
            self.assertFalse(mt.skip_ahead(self.queue(), st))
        merge.assert_not_called()
        sh.assert_not_called()  # later PRs are never updated or failed by the look-ahead

    def test_one_pass_tracks_queue_head_and_skips_ahead(self):
        q = self.queue()
        st = {}
        with mock.patch.object(mt, "fetch_queue", return_value=q), \
                mock.patch.object(mt, "check_dev_red"), \
                mock.patch.object(mt, "paused_reason", return_value=None), \
                mock.patch.object(mt, "git", return_value=""), \
                mock.patch.object(mt, "process_head", return_value=False), \
                mock.patch.object(mt, "skip_ahead", return_value=False) as sk:
            mt.one_pass(st)
            first = st["head_since"]
            mt.one_pass(st)
        self.assertEqual(st["head_pr"], 1)
        self.assertEqual(st["head_since"], first)  # unchanged while #1 stays at the front
        self.assertEqual(sk.call_count, 2)


class CancelledTest(Base):
    NAME = "full / macOS arm64"

    def runs(self, cid, minutes_ago=5):
        done = (mt.now_utc() - mt.dt.timedelta(minutes=minutes_ago)).strftime("%Y-%m-%dT%H:%M:%SZ")
        return {self.NAME: {"id": cid, "name": self.NAME, "status": "completed",
                            "conclusion": "cancelled", "completed_at": done, "html_url": "u"}}

    def rerun(self, ps, runs, rc=0, infra=False):
        calls = []

        def fake_sh(cmd, **kw):
            calls.append(cmd)
            return (rc, "", "busy")
        with mock.patch.object(mt, "check_runs", return_value=runs), \
                mock.patch.object(mt, "runner_not_acquired", return_value=infra), \
                mock.patch.object(mt, "sh", side_effect=fake_sh):
            why = mt.rerun_cancelled(pr(), ps, [self.NAME])
        return why, calls

    def test_first_cancel_reruns_job_second_cancel_fails(self):
        ps = {}
        why, calls = self.rerun(ps, self.runs(101))
        self.assertIsNone(why)
        self.assertEqual(calls[0][:4], ["gh", "api", "-X", "POST"])
        self.assertTrue(calls[0][4].endswith("/actions/jobs/101/rerun"))
        why, calls = self.rerun(ps, self.runs(101))     # same cancelled run: just wait
        self.assertIsNone(why)
        self.assertEqual(calls, [])
        why, calls = self.rerun(ps, self.runs(102))     # the rerun was cancelled too
        self.assertIn("2 次", why)
        self.assertEqual(calls, [])

    def test_no_runner_cancels_are_rerun_without_counting(self):
        ps = {}
        for cid in (201, 202, 203):  # GitHub capacity: never counted toward the limit
            why, calls = self.rerun(ps, self.runs(cid), infra=True)
            self.assertIsNone(why)
            self.assertEqual(len(calls), 1)
        why, calls = self.rerun(ps, self.runs(203), infra=True)  # same run: wait
        self.assertEqual(calls, [])
        self.rerun(ps, self.runs(204))                             # a real cancel: rerun, 1/2
        why, _ = self.rerun(ps, self.runs(205))                    # second real cancel: fail
        self.assertIn("2 次", why)

    def test_runner_not_acquired_reads_annotations(self):
        notes = [{"message": "The job was not acquired by Runner of type hosted even after multiple attempts"}]
        with mock.patch.object(mt, "gh_json", return_value=notes):
            self.assertTrue(mt.runner_not_acquired(1))
        with mock.patch.object(mt, "gh_json", return_value=[{"message": "Canceling since a higher priority waiting request exists"}]):
            self.assertFalse(mt.runner_not_acquired(1))

    def test_fresh_cancel_waits_for_replacing_run(self):
        why, calls = self.rerun({}, self.runs(101, minutes_ago=0))
        self.assertIsNone(why)
        self.assertEqual(calls, [])

    def test_rerun_refused_is_retried_next_pass(self):
        ps = {}
        why, calls = self.rerun(ps, self.runs(101), rc=1)  # run still has other jobs going
        self.assertIsNone(why)
        self.assertNotIn(101, ps["cancelled_runs"].get(self.NAME, []))
        why, calls = self.rerun(ps, self.runs(101), rc=0)
        self.assertEqual(len(calls), 1)
        self.assertEqual(ps["cancelled_runs"][self.NAME], [101])

    def test_process_head_does_not_fail_on_cancel(self):
        checks = [(self.NAME, "cancelled", "u"), ("full / Windows x64", "success", "u")]
        with mock.patch.object(mt, "decide", return_value=("wait-ci", "等 CI", {"checks": checks})), \
                mock.patch.object(mt, "rerun_cancelled", return_value=None) as rr, \
                mock.patch.object(mt, "fail_pr") as fail, \
                mock.patch.object(mt, "nudge_ci") as nudge:
            self.assertFalse(mt.process_head(pr(), {}))
        rr.assert_called_once()
        fail.assert_not_called()
        nudge.assert_not_called()

    def test_new_head_resets_cancel_count(self):
        st = {"prs": {"7": {"head": "old", "since": mt.now_utc().isoformat(),
                            "cancelled_runs": {self.NAME: [1]}}}}
        with mock.patch.object(mt, "decide", return_value=("wait-ci", "x", {"checks": []})):
            mt.process_head(pr(headRefOid="new"), st)
        self.assertNotIn("cancelled_runs", st["prs"]["7"])


class BoardTest(Base):
    def test_handoff_section_preserved(self):
        old = "x\n%s\n谁在做什么：A 在写 B\n%s\ny" % (mt.HANDOFF_START, mt.HANDOFF_END)
        wt = [{"host": "local", "name": "bae-03", "branch": "core/x", "comment": "c|d", "status": "",
               "last_activity": 1, "agents": [{"orcaSessionId": "aaaabbbbcccc", "state": "working"}]}]
        prs = [{"number": 9, "title": "T", "url": "u", "isDraft": False, "headRefName": "core/x",
                "baseRefName": "dev", "labels": {"nodes": [{"name": "blocked"}]},
                "comments": {"nodes": [{"body": "blocked: 等 #5 合并", "createdAt": ""}]},
                "commits": {"nodes": [{"commit": {"oid": "o", "committedDate": "", "statusCheckRollup": {
                    "contexts": {"nodes": [{"name": "fast / macOS arm64", "status": "COMPLETED",
                                            "conclusion": "SUCCESS"}]}}}}]}}]
        with mock.patch.object(mt, "orca_worktrees", return_value=wt), \
                mock.patch.object(mt, "gh_open_prs", return_value=prs), \
                mock.patch.object(mt, "branch_dates", return_value={}), \
                mock.patch.object(mt, "git", return_value="abc\n"), \
                mock.patch.object(mt, "check_runs", return_value={}):
            body = mt.build_board({"queue": [], "mode": "dry-run"}, old)
        self.assertIn("谁在做什么：A 在写 B", body)
        self.assertIn("⛔ 等 #5 合并", body)
        self.assertIn("c\\|d", body)
        self.assertIn("#9 Ready", body)
        self.assertEqual(body.count(mt.HANDOFF_START), 1)


class SkipAheadOverlapTest(Base):
    def test_ready_pr_sharing_files_with_head_does_not_jump(self):
        repo = self.make_repo()
        heads = {}
        for name, files in (("head", {"crates/fx/lib.rs": "h\n", "docs/a.md": "h\n"}),
                            ("same", {"crates/fx/lib.rs": "s\n"}),
                            ("docs", {"docs/a.md": "d\n", "Cargo.lock": "d\n"}),
                            ("other", {"crates/other/x.rs": "o\n"})):
            run(repo, "git", "checkout", "-q", "-b", name, "dev")
            heads[name] = self.commit(repo, files, name)
        run(repo, "git", "checkout", "-q", "dev")
        q = [pr(number=1, headRefName="head", headRefOid=heads["head"]),
             pr(number=2, headRefName="same", headRefOid=heads["same"]),
             pr(number=3, headRefName="docs", headRefOid=heads["docs"]),
             pr(number=4, headRefName="other", headRefOid=heads["other"])]
        old = (mt.now_utc() - mt.dt.timedelta(minutes=45)).isoformat()
        with mock.patch.object(mt, "decide", return_value=("merge", "ok", {})), \
                mock.patch.object(mt, "fetch_pr_head"), \
                mock.patch.object(mt, "do_merge", return_value=True) as merge:
            st = {"head_pr": 1, "head_since": old}
            self.assertTrue(mt.skip_ahead(q, st))
            # #2 shares crates/fx/lib.rs with the head: waits. #3 only shares docs/ and
            # Cargo.lock, which never count: it may go first.
            self.assertEqual(merge.call_args[0][0]["number"], 3)
            self.assertEqual(st["prs"]["2"]["decision"], "wait-head")
            self.assertIn("crates/fx/lib.rs", st["prs"]["2"]["detail"])
            merge.reset_mock()
            self.assertTrue(mt.skip_ahead([q[0], q[1], q[3]], {"head_pr": 1, "head_since": old}))
            self.assertEqual(merge.call_args[0][0]["number"], 4)
            merge.reset_mock()
            self.assertFalse(mt.skip_ahead(q[:2], {"head_pr": 1, "head_since": old}))
            merge.assert_not_called()


class CrateRelationTest(Base):
    """Overlap by crate dependency: #303 changed bae-render's LayerDependency, #299 built
    that struct in bae-effects; no shared file, but together dev did not compile."""

    def rel(self, a, b):
        return mt.crate_relation(set(a), set(b))

    def test_dependency_in_either_direction_is_related(self):
        self.assertIn("fx 依赖 render", self.rel(["crates/fx/src/a.rs"], ["crates/render/src/layer.rs"]))
        self.assertIn("fx 依赖 render", self.rel(["crates/render/src/layer.rs"], ["crates/fx/src/a.rs"]))
        self.assertIn("同一个 crate：ui", self.rel(["crates/ui/a.rs"], ["crates/ui/b.rs"]))

    def test_unrelated_crates_docs_tools_github_are_not(self):
        self.assertEqual(self.rel(["crates/fx/a.rs"], ["crates/ui/b.rs"]), "")
        for other in (["docs/spec/x.md"], ["tools/merge-train/merge_train.py"],
                      [".github/workflows/ci.yml"], ["Cargo.lock"], ["README.md"]):
            self.assertEqual(self.rel(["crates/fx/a.rs"], other), "", other)

    def test_docs_read_by_code_and_root_manifest_count(self):
        self.assertIn("ui", self.rel(["docs/ui/menu.md"], ["crates/ui/a.rs"]))
        self.assertIn("所有 crate", self.rel(["Cargo.toml"], ["crates/other/a.rs"]))

    def test_unknown_graph_is_treated_as_related(self):
        with mock.patch.object(mt, "crate_graph", side_effect=RuntimeError("no cargo")):
            self.assertIn("算不出来", self.rel(["crates/fx/a.rs"], ["crates/ui/b.rs"]))

    def test_overlap_reports_crate_dependency_without_shared_files(self):
        repo = self.make_repo()
        run(repo, "git", "checkout", "-qb", "feat")
        pr_head = self.commit(repo, {"crates/fx/src/stack.rs": "uses LayerDependency\n"}, "pr")
        run(repo, "git", "checkout", "-q", "dev")
        self.commit(repo, {"crates/render/src/layer.rs": "LayerDependency changed\n"}, "dev")
        run(repo, "git", "push", "-q", "origin", "dev")
        run(repo, "git", "fetch", "-q", "origin")
        # File-level only (the repo default since 2026-11-24): different files, no overlap.
        with mock.patch.dict(mt.CFG["merge"], {"relate_crates": False}):
            self.assertEqual(mt.overlap(pr_head), [])
        with mock.patch.dict(mt.CFG["merge"], {"relate_crates": True}):
            over = mt.overlap(pr_head)
        self.assertEqual(len(over), 1)
        self.assertIn("fx 依赖 render", over[0])
        # After update-branch the merge-base is dev's head: nothing to relate any more,
        # even if the graph cannot be computed (no update-branch loop).
        run(repo, "git", "checkout", "-q", "feat")
        run(repo, "git", "merge", "-q", "--no-edit", "origin/dev")
        merged = run(repo, "git", "rev-parse", "HEAD")
        with mock.patch.object(mt, "crate_graph", side_effect=RuntimeError("no cargo")), \
                mock.patch.dict(mt.CFG["merge"], {"relate_crates": True}):
            self.assertEqual(mt.overlap(merged), [])

    def test_related_pr_does_not_skip_ahead(self):
        q = [pr(number=1, headRefOid="h1"), pr(number=2, headRefOid="h2"), pr(number=3, headRefOid="h3")]
        files = {"h1": {"crates/render/src/layer.rs"}, "h2": {"crates/fx/src/a.rs"}, "h3": {"crates/ui/a.rs"}}
        st = {"head_pr": 1, "head_since": (mt.now_utc() - mt.dt.timedelta(minutes=40)).isoformat()}
        with mock.patch.object(mt, "decide", return_value=("merge", "ok", {})), \
                mock.patch.object(mt, "fetch_pr_head"), \
                mock.patch.object(mt, "changed_files", side_effect=lambda sha: files[sha]), \
                mock.patch.object(mt, "do_merge", return_value=True) as merge, \
                mock.patch.dict(mt.CFG["merge"], {"relate_crates": True}):
            self.assertTrue(mt.skip_ahead(q, st))
        self.assertEqual(merge.call_args[0][0]["number"], 3)
        self.assertIn("fx 依赖 render", st["prs"]["2"]["detail"])

    def test_without_relate_crates_only_files_block_skip_ahead(self):
        q = [pr(number=1, headRefOid="h1"), pr(number=2, headRefOid="h2")]
        files = {"h1": {"crates/render/src/layer.rs"}, "h2": {"crates/render/src/other.rs"}}
        st = {"head_pr": 1, "head_since": (mt.now_utc() - mt.dt.timedelta(minutes=40)).isoformat()}
        with mock.patch.object(mt, "decide", return_value=("merge", "ok", {})), \
                mock.patch.object(mt, "fetch_pr_head"), \
                mock.patch.object(mt, "changed_files", side_effect=lambda sha: files[sha]), \
                mock.patch.object(mt, "do_merge", return_value=True) as merge, \
                mock.patch.dict(mt.CFG["merge"], {"relate_crates": False}):
            self.assertTrue(mt.skip_ahead(q, st))
        self.assertEqual(merge.call_args[0][0]["number"], 2)


class DevCancelledTest(Base):
    def run_check(self, st, conclusion="cancelled", minutes_ago=5, rc=0, cid=900):
        done = (mt.now_utc() - mt.dt.timedelta(minutes=minutes_ago)).strftime("%Y-%m-%dT%H:%M:%SZ")
        latest = {n: {"id": cid, "name": n, "status": "completed", "conclusion": conclusion,
                      "completed_at": done, "html_url": "u"} for n in mt.CFG["checks"]["full"][:1]}
        calls = []

        def fake_sh(cmd, **kw):
            calls.append(cmd)
            return (rc, "", "run in progress")
        with mock.patch.object(mt, "sh", side_effect=fake_sh):
            mt.rerun_dev_cancelled(st, "d" * 40, latest)
        return [c for c in calls if c[:4] == ["gh", "api", "-X", "POST"]]

    def test_cancelled_dev_job_is_rerun_until_it_has_a_result(self):
        st = {}
        self.assertEqual(len(self.run_check(st)), 1)          # rerun
        self.assertEqual(self.run_check(st), [])               # same job: already rerun
        self.assertEqual(len(self.run_check(st, cid=901)), 1)  # cancelled again: rerun again
        self.assertEqual(self.run_check(st, conclusion="success", cid=902), [])

    def test_waits_for_grace_and_retries_when_refused(self):
        st = {}
        self.assertEqual(self.run_check(st, minutes_ago=0), [])
        self.assertEqual(len(self.run_check(st, rc=1)), 1)  # GitHub refused (run still going)
        self.assertEqual(len(self.run_check(st, rc=0)), 1)  # retried next pass
        self.assertEqual(self.run_check(st), [])

    def test_check_dev_red_reruns_before_ack_shortcut(self):
        with mock.patch.object(mt, "git", return_value="d" * 40 + "\n"), \
                mock.patch.object(mt, "check_runs", return_value={}), \
                mock.patch.object(mt, "red_ack_get", return_value="d" * 40), \
                mock.patch.object(mt, "rerun_dev_cancelled") as rr:
            self.assertFalse(mt.check_dev_red({}))
        rr.assert_called_once()


class AutoResumeTest(Base):
    def latest(self, conclusion="success", status="completed"):
        return {n: {"id": i, "name": n, "status": status, "conclusion": conclusion,
                    "completed_at": "2026-11-24T00:00:00Z", "html_url": "u"}
                for i, n in enumerate(mt.CFG["checks"]["full"])}

    def check(self, st, sha, latest, ack=""):
        with mock.patch.object(mt, "git", return_value=sha + "\n"), \
                mock.patch.object(mt, "check_runs", return_value=latest), \
                mock.patch.object(mt, "board_issue_number", return_value=250), \
                mock.patch.object(mt, "sh", return_value=(0, "", "")) as sh, \
                mock.patch.object(mt, "send") as send:
            mt.check_dev_red(st)
        return sh, send

    def test_red_dev_pauses_and_green_dev_resumes_with_one_line(self):
        st = {}
        red, green = "a" * 40, "b" * 40
        _, send = self.check(st, red, self.latest("failure"))
        self.assertTrue(os.path.exists(mt.pause_file()))
        self.assertEqual(mt.auto_paused_sha(), red)
        self.assertEqual(send.call_count, len(mt.LEADERS["pause_notify"]))
        # dev still running on the fix: stay paused
        _, send = self.check(st, green, self.latest(None, "in_progress"))
        self.assertTrue(os.path.exists(mt.pause_file()))
        send.assert_not_called()
        # the fix is green: resume, drop the label, tell both leaders in one line
        sh, send = self.check(st, green, self.latest("success"))
        self.assertFalse(os.path.exists(mt.pause_file()))
        self.assertEqual(mt.auto_paused_sha(), "")
        self.assertTrue(any("--remove-label" in c[0][0] for c in sh.call_args_list))
        self.assertEqual([c[0][0] for c in send.call_args_list], mt.LEADERS["pause_notify"])
        self.assertIn("自动恢复", send.call_args[0][1])
        self.assertIn(green[:7], send.call_args[0][1])

    def test_manual_pause_is_not_lifted(self):
        mt.ARGS.reason = "等 #123"
        with mock.patch.object(mt, "board_issue_number", return_value=None):
            mt.cmd_pause()
        sh, send = self.check({}, "c" * 40, self.latest("success"))
        self.assertTrue(os.path.exists(mt.pause_file()))
        send.assert_not_called()

    def test_manual_pause_over_auto_pause_wins(self):
        st = {}
        self.check(st, "a" * 40, self.latest("failure"))
        mt.ARGS.reason = "人为暂停"
        with mock.patch.object(mt, "board_issue_number", return_value=None):
            mt.cmd_pause()
        _, send = self.check(st, "b" * 40, self.latest("success"))
        self.assertTrue(os.path.exists(mt.pause_file()))
        send.assert_not_called()

    def test_dry_run_only_reports(self):
        st = {}
        self.check(st, "a" * 40, self.latest("failure"))
        mt.ARGS.dry_run = True
        sh, send = self.check(st, "b" * 40, self.latest("success"))
        self.assertTrue(os.path.exists(mt.pause_file()))
        send.assert_not_called()


if __name__ == "__main__":
    unittest.main()
