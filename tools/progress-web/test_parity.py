import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import server


class ParityTest(unittest.TestCase):
    def test_open_close_rename_and_repeat_preserve_manual_reports(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(server, "DB", Path(tmp) / "board.db"):
            server.initialize()
            issue = dict(number=401, title="发现交互差异", state="open", updated_at="2026-11-25T00:00:00Z")
            umbrella = dict(issue, number=400, title="用户反馈")
            def sync():
                batch = json.dumps([issue, umbrella, dict(issue, number=402, pull_request={})]).encode()
                with patch("urllib.request.urlopen", return_value=io.BytesIO(batch)):
                    server.sync_parity()
            sync()
            state = server.snapshot()
            self.assertEqual([len(x["tasks"]) for x in state["parity"]], [7, 1])
            def progress():
                return {x["id"]:x["status"] for x in server.snapshot()["items"] if x["id"].startswith("ae-401")}
            self.assertEqual(progress(), {"ae-401-fix":"todo", "ae-401-verify":"todo"})
            server.report(dict(item="ae-401-verify", status="doing", agent="tester"))
            issue.update(state="closed", title="更新标题")
            sync()
            self.assertEqual(progress(), {"ae-401-fix":"todo", "ae-401-verify":"doing"})
            checked = server.snapshot()
            self.assertEqual(checked["parity"][1]["title"], "更新标题")
            sync()
            server.initialize()
            self.assertEqual(checked, server.snapshot())
