import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import server


class LegacyEffectsTest(unittest.TestCase):
    def test_upgrade_and_report_retries(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(server, "DB", Path(tmp) / "test.db"):
            server.initialize()
            reference = server.HERE.parents[1] / "docs/reference/ae/effects"
            expected = {"fx:" + fx["matchName"] for path in reference.glob("obsolete*.json")
                        for fx in json.loads(path.read_text())["effects"]}
            items = server.snapshot()["items"]
            legacy = [row for row in items if row["module"] == "旧版效果"]
            self.assertEqual({row["id"] for row in legacy}, expected)
            batch = [row for row in legacy if row["prs"] in ([393], [394], [396])]
            self.assertEqual(len(batch), 19)
            self.assertTrue(all(row["status"] == "todo" for row in batch))

            first = batch[0]["id"]
            server.report(dict(item=first, agent="tester", status="doing", note="keep", pr=999))
            with server.closing(server.connect()) as db, db:
                db.execute("delete from items where module='旧版效果' and id<>?", (first,))
                db.execute("insert into pulls values(999,null,'keep','OPEN','2026-11-24')")
            before = server.snapshot()
            server.initialize()
            after = server.snapshot()
            old = {row["id"]: row for row in before["items"]}
            self.assertEqual(old, {row["id"]: row for row in after["items"] if row["id"] in old})
            self.assertEqual(before["activity"], after["activity"])
            self.assertEqual(before["agents"], after["agents"])
            server.initialize()
            self.assertEqual(after, server.snapshot())
            for row in batch:
                data = dict(item=row["id"], agent="tester", status="done", pr=row["prs"][0])
                self.assertTrue(server.report(data))
                self.assertFalse(server.report(data))
            with self.assertRaises(ValueError):
                server.report(dict(item="fx:ADBE Unknown Alias", agent="tester", status="done"))


if __name__ == "__main__":
    unittest.main()
