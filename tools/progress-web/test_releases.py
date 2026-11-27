import sqlite3
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import server


class ReleasesTest(unittest.TestCase):
    def test_upgrade_claims_reported_items_and_groups_new_bugs(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(server, "DB", Path(tmp) / "board.db"):
            # A 0.1.0 database: no release column, a bug reported under 新增能力.
            db = sqlite3.connect(server.DB)
            db.execute("""create table items(id text primary key, kind text, module text, name text,
                status text, tag text, prs text, agent text default '', updated text, note text default '')""")
            db.execute("""insert into items(id,kind,module,name,status,tag,prs,agent,updated)
                values('bug-drag-feedback','milestone','新增能力','bug-drag-feedback','doing','','[836]','A','1')""")
            db.commit()
            db.close()
            server.initialize()
            items = {x["id"]: x for x in server.snapshot()["items"]}
            bug = items["bug-drag-feedback"]
            self.assertEqual(bug["release"], "0.2.0")
            self.assertEqual(bug["module"], "用户实测 bug")
            self.assertEqual(bug["name"], "拖放反馈：拖动轮廓、落点提示、禁止光标")
            self.assertEqual(bug["prs"], [836], "reports and PRs are kept")
            self.assertEqual(items["layers"]["release"], "0.1.0")
            self.assertEqual(items["m2-agent-mcp"]["module"], "M2 交接与协同")
            # A new user bug joins the current release's bug group.
            server.report(dict(item="bug-new-thing", agent="A", status="doing"))
            new = {x["id"]: x for x in server.snapshot()["items"]}["bug-new-thing"]
            self.assertEqual((new["release"], new["module"]), ("0.2.0", "用户实测 bug"))
            # Restarting keeps it all.
            state = server.snapshot()
            self.assertEqual(state["history"], ["0.1.0"])
            server.initialize()
            self.assertEqual(server.snapshot()["items"], state["items"])


if __name__ == "__main__":
    unittest.main()
