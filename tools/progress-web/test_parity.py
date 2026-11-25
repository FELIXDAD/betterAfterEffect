from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import server


class ParityTest(unittest.TestCase):
    def test_fix_lifecycle_and_independent_acceptance(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(server, "DB", Path(tmp) / "board.db"):
            server.initialize()
            issue = dict(number=405, title="时间轴滚动", state="open", updated_at="1")
            umbrella = dict(issue, number=400, title="用户反馈")
            pr = dict(number=420, body="Fixes #405", title="patch", merged_at=None)
            scoped = dict(pr, number=421, body="Fixes: ae-400-3-fix", merged_at="2")
            events = {400: [], 405: []}
            def sync():
                for record in (pr, scoped):
                    record.update(status="done" if record["merged_at"] else "ready", updated_at="1")
                for record in (issue, umbrella):
                    record.update(parity=True, events=events[record["number"]])
                server.apply_github(dict(pulls=[pr, scoped], issues=[issue, umbrella]))
                return {x["id"]:x for x in server.snapshot()["items"]}
            rows = sync()
            self.assertEqual(rows["ae-405-fix"]["module"], "时间轴")
            self.assertEqual(rows["ae-405-fix"]["status"], "todo")
            self.assertEqual([rows[f"ae-400-{i}-fix"]["prs"] for i in range(1,8)],
                             [[], [], [421], [], [], [], []])
            self.assertEqual([rows[f"ae-400-{i}-fix"]["status"] for i in range(1, 8)],
                             ["todo", "todo", "done", "todo", "todo", "todo", "todo"])
            server.report(dict(item="ae-405-verify",agent="tester",status="doing"))
            issue["state"] = "closed"
            rows = sync()
            self.assertEqual((rows["ae-405-fix"]["status"], rows["ae-405-fix"]["prs"]), ("done", [420]))
            issue["state"] = "open"
            events[405].append(dict(event="reopened", created_at="3"))
            self.assertEqual(sync()["ae-405-fix"]["status"], "todo")
            pr["merged_at"] = "4"
            rows = sync()
            self.assertEqual((rows["ae-405-fix"]["status"], rows["ae-405-fix"]["prs"]), ("done", [420]))
            events[405].append(dict(event="reopened", created_at="5"))
            self.assertEqual(sync()["ae-405-fix"]["status"], "todo")
            issue.update(body="模块：工作区\n修复 PR: #421")
            scoped["merged_at"] = "6"
            rows = sync()
            self.assertEqual(rows["ae-405-fix"]["status"], "done")
            self.assertEqual(rows["ae-405-fix"]["prs"], [420,421])
            self.assertEqual(rows["ae-405-fix"]["module"], "工作区")
            self.assertEqual(rows["ae-405-verify"]["status"], "doing")
            checked = server.snapshot()
            sync()
            server.initialize()
            self.assertEqual(checked, server.snapshot())


    def test_close_event_sha_and_explicit_open_pr(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(server, "DB", Path(tmp) / "db"):
            server.initialize()
            issue = dict(number=444, title="工作区", state="closed", updated_at="1")
            events = [dict(event="closed", created_at="1", commit_id="abc")]
            pr = dict(number=445, merged_at="1", merge_commit_sha="abc")
            with server.closing(server.connect()) as db, db:
                server.reconcile(db, issue, [pr], events)
            task = server.snapshot()["parity"][-1]["tasks"][0]
            self.assertEqual(task["merged"], [445])
            issue["state"] = "open"
            events.append(dict(event="reopened", created_at="2"))
            with server.closing(server.connect()) as db, db:
                server.reconcile(db, issue, [pr], events)
            server.report(dict(item=task["fix"], agent="developer", status="done", pr=446))
            pending = dict(number=446, merged_at=None, merge_commit_sha=None)
            with server.closing(server.connect()) as db, db:
                server.reconcile(db, issue, [pr, pending], events)
                self.assertEqual(db.execute("select status from items where id=?", (task["fix"],)).fetchone()[0], "todo")
            self.assertEqual(server.snapshot()["parity"][-1]["tasks"][0]["merged"], [445])
            pending["merged_at"] = "3"
            with server.closing(server.connect()) as db, db:
                server.reconcile(db, issue, [pr, pending], events)
                self.assertEqual(db.execute("select status from items where id=?", (task["fix"],)).fetchone()[0], "done")
            self.assertEqual(server.snapshot()["parity"][-1]["tasks"][0]["merged"], [445,446])

    def test_milestone_state_is_derived_from_all_linked_prs(self):
        states = {1:"done", 2:"done", 3:"doing", 4:"ready", 5:"closed"}
        for prs, expected in (([], "todo"), ([1,2], "done"), ([1,3], "doing"), ([3,4], "doing"),
                              ([1,5], "done"), ([5], "todo"), ([4,5], "ready"), ([1,999], "todo")):
            self.assertEqual(server.milestone_status(prs, states), expected)

    def test_partial_sync_keeps_stored_records(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(server, "DB", Path(tmp) / "db"), patch.object(server, "effects", return_value=[]):
            server.initialize()
            issue = dict(number=600, title="layers", state="open", updated_at="1",
                         body="能力 ID: layers", parity=False, labels=[])
            superseded = dict(number=700, status="closed", updated_at="1", body="Fixes #600", merged_at=None)
            replacement = dict(number=701, status="done", updated_at="2", body="Fixes #600", merged_at="2")
            def layers():
                return next(x for x in server.snapshot()["items"] if x["id"] == "layers")
            server.apply_github(dict(pulls=[superseded], issues=[issue]))
            self.assertEqual((layers()["prs"], layers()["status"]), ([700], "todo"))
            # Later syncs carry only what changed; the issue and earlier links must survive.
            server.apply_github(dict(pulls=[replacement], issues=[]))
            self.assertEqual((layers()["prs"], layers()["status"]), ([700, 701], "done"))

    def test_recent_work_queue_and_last_activity(self):
        from datetime import datetime, timedelta, timezone
        with tempfile.TemporaryDirectory() as tmp, patch.object(server, "DB", Path(tmp) / "board.db"), patch.object(server, "effects", return_value=[]):
            server.initialize()
            clock = datetime.now(timezone.utc)
            old = (clock-timedelta(hours=3)).isoformat()
            fresh = (clock-timedelta(minutes=10)).isoformat()
            pr = dict(number=324, status="ready", updated_at=fresh, committed_at=old,
                      commit_author="developer", queued=True)
            def sync():
                server.apply_github(dict(pulls=[pr],issues=[]))
                state = server.snapshot()
                self.assertEqual(state["merge_queue"], int(pr["queued"] and pr["status"] in ("ready","doing")))
                return next(x for x in state["items"] if x["id"] == "shape-ops")
            self.assertEqual(sync()["status"], "ready")
            pr["committed_at"] = fresh
            item = sync()
            self.assertEqual(item["status"], "doing")
            self.assertEqual(item["last_activity"]["agent"], "developer")
            self.assertEqual(item["pr_counts"], {"done":0,"ready":1,"doing":0})
            pr["committed_at"] = old
            server.report(dict(item="shape-ops",agent="worker",status="doing"))
            item = sync()
            self.assertEqual(item["last_activity"]["agent"], "worker")
            self.assertEqual(item["status"], "doing")
            pr.update(status="done", queued=False)
            self.assertEqual(sync()["status"], "done")
            server.report(dict(item="no-pr",agent="worker",status="doing"))
            self.assertEqual(next(x for x in server.snapshot()["items"] if x["id"]=="no-pr")["status"], "doing")
