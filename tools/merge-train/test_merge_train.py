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

    def tearDown(self):
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
        for rule in leaders["rules"]:
            self.assertIn(rule["leader"], leaders["leaders"])


class OverlapTest(Base):
    def test_empty_on_both_sides_is_empty(self):
        repo = self.make_repo()
        head = run(repo, "git", "rev-parse", "HEAD")  # PR with no changes, dev unchanged
        self.assertEqual(mt.overlap(head), [])

    def test_overlap_and_ignores(self):
        repo = self.make_repo()
        run(repo, "git", "checkout", "-qb", "feat")
        pr = self.commit(repo, {"a.txt": "pr\n", "docs/x.md": "pr\n", "Cargo.lock": "pr\n"}, "pr")
        run(repo, "git", "checkout", "-q", "dev")
        self.commit(repo, {"docs/x.md": "dev\n", "Cargo.lock": "dev\n", "b.txt": "b\n"}, "dev1")
        run(repo, "git", "push", "-q", "origin", "dev")
        run(repo, "git", "fetch", "-q", "origin")
        self.assertEqual(mt.overlap(pr), [])  # docs/ and Cargo.lock never count
        self.commit(repo, {"a.txt": "dev\n"}, "dev2")
        run(repo, "git", "push", "-q", "origin", "dev")
        run(repo, "git", "fetch", "-q", "origin")
        self.assertEqual(mt.overlap(pr), ["a.txt"])


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
                          (("completed", "cancelled"), "failed"), (("completed", "failure"), "failed")):
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
        self.assertEqual(send.call_args[0][0], "09fe9cc0")
        self.assertIn("#7", send.call_args[0][1])
        self.assertEqual(st["merged"][-1]["dev"], "abc1234")

    def test_code_pr_uses_merge_commit(self):
        self.assertFalse(mt.squash_wanted(pr()))
        self.assertTrue(mt.squash_wanted(pr(labels=["reference-data"])))

    def test_ci_failure_drops_label_and_notifies(self):
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
        self.assertEqual([c[0][0] for c in send.call_args_list], ["orca_session_id:aaaa", "ca3a6511"])

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
        self.assertEqual(mt.leader_for(pr(headRefName="ui/design"), milan), "milan")
        self.assertEqual(mt.leader_for(pr(headRefName="fix/226"), local12), "09fe9cc0")
        self.assertEqual(mt.leader_for(pr(headRefName="ui/x"), None), "09fe9cc0")
        self.assertEqual(mt.leader_for(pr(headRefName="core/x"), None), "ca3a6511")


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
        self.wait(pr(), st, self.MISSING, 11).assert_called_once()     # nudge
        st["prs"]["7"]["nudged_head"] = "h1"
        self.wait(pr(), st, self.MISSING, 30).assert_not_called()     # once per head
        self.wait(pr(), {}, self.RUNNING, 60).assert_not_called()     # CI is running: wait

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


if __name__ == "__main__":
    unittest.main()
