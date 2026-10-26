import pathlib
import sqlite3
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
MIGRATIONS = ROOT / "crates/lattice-desktop/migrations"
TARGET = MIGRATIONS / "20261025120000_fts_rowid_maintenance.sql"


class FtsMaintenanceTest(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        self.db.execute("PRAGMA foreign_keys=ON")
        before = sorted(p for p in MIGRATIONS.glob("*.sql") if p.name < TARGET.name)
        for migration in before:
            self.db.executescript("BEGIN;\n" + migration.read_text() + "\nCOMMIT;")
        self.db.execute("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum) VALUES('d','/d','d',1,'now','x')")
        self.db.execute("INSERT INTO text_chunks(id,document_id,content,context_prefix,chunk_index) VALUES('c1','d','alpha original','Heading',0)")
        self.db.execute("INSERT INTO text_chunks(id,document_id,content,context_prefix,chunk_index) VALUES('c2','d','beta sibling','Other',1)")
        self.db.execute("INSERT INTO conversations(id,title) VALUES('v1','Title original')")
        self.db.execute("INSERT INTO conversation_messages(id,conversation_id,role,content) VALUES('m1','v1','user','message original')")
        self.db.execute("INSERT INTO conversation_message_bookmarks(id,conversation_id,message_id,title,note) VALUES('b1','v1','m1','bookmark original','note')")
        self.db.commit()
        for migration in sorted(p for p in MIGRATIONS.glob("*.sql") if p.name >= TARGET.name):
            self.db.executescript("BEGIN;\n" + migration.read_text() + "\nCOMMIT;")

    def tearDown(self):
        self.db.close()

    def matches(self, table, query):
        return self.db.execute(f"SELECT COUNT(*) FROM {table} WHERE {table} MATCH ?", (query,)).fetchone()[0]

    def test_backfill_updates_deletes_cascades_and_vacuum(self):
        identities = self.db.execute("SELECT * FROM chunk_fts_rowids ORDER BY fts_rowid").fetchall()
        source_identities = self.db.execute("SELECT * FROM conversation_search_fts_rowids ORDER BY fts_rowid").fetchall()
        self.db.execute("VACUUM")
        self.assertEqual(self.db.execute("SELECT * FROM chunk_fts_rowids ORDER BY fts_rowid").fetchall(), identities)
        self.assertEqual(self.db.execute("SELECT * FROM conversation_search_fts_rowids ORDER BY fts_rowid").fetchall(), source_identities)
        self.assertEqual(self.matches("chunks_fts", "alpha"), 1)
        self.assertEqual(self.matches("chunks_trigram", "alpha"), 1)
        self.assertEqual(self.matches("conversation_search_fts", "original"), 3)
        self.db.execute("UPDATE text_chunks SET content='gamma revised', id='c1-new' WHERE id='c1'")
        self.assertEqual(self.matches("chunks_fts", "gamma"), 1)
        self.assertEqual(self.matches("chunks_fts", "alpha"), 0)
        self.db.execute("UPDATE conversation_messages SET content='message revised' WHERE id='m1'")
        self.assertEqual(self.matches("conversation_search_fts", "revised"), 1)
        self.db.execute("UPDATE conversations SET title='Title revised' WHERE id='v1'")
        self.assertEqual(self.matches("conversation_search_fts", "revised"), 2)
        self.db.execute("UPDATE conversation_message_bookmarks SET title='bookmark revised', id='b2' WHERE id='b1'")
        self.assertEqual(self.matches("conversation_search_fts", "bookmark"), 1)
        self.assertEqual(self.matches("conversation_search_fts", "revised"), 3)
        self.db.execute("DELETE FROM conversation_message_bookmarks WHERE id='b2'")
        self.assertEqual(self.db.execute("SELECT count(*) FROM conversation_search_fts_rowids WHERE source='bookmark'").fetchone()[0], 0)
        self.assertEqual(self.matches("conversation_search_fts", "bookmark"), 0)
        self.db.execute("DELETE FROM documents WHERE id='d'")
        self.assertEqual(self.db.execute("SELECT count(*) FROM chunks_fts").fetchone()[0], 0)
        self.assertEqual(self.db.execute("SELECT count(*) FROM chunks_trigram").fetchone()[0], 0)
        self.assertEqual(self.db.execute("SELECT count(*) FROM chunk_fts_rowids").fetchone()[0], 0)
        self.db.execute("DELETE FROM conversations WHERE id='v1'")
        self.assertEqual(self.db.execute("SELECT count(*) FROM conversation_search_fts").fetchone()[0], 0)
        self.assertEqual(self.db.execute("SELECT count(*) FROM conversation_search_fts_rowids").fetchone()[0], 0)
        self.db.commit()
        self.db.execute("VACUUM")
        self.db.execute("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum) VALUES('d2','/d2','d2',1,'now','y')")
        self.db.execute("INSERT INTO text_chunks(id,document_id,content,context_prefix,chunk_index) VALUES('c3','d2','after vacuum','H',0)")
        self.assertEqual(self.matches("chunks_fts", "vacuum"), 1)
        self.assertEqual(self.matches("chunks_trigram", "vacuum"), 1)
        self.assertEqual(self.db.execute("PRAGMA foreign_key_check").fetchall(), [])

    def test_new_source_kinds_can_share_the_same_text_identifier(self):
        self.db.execute("INSERT INTO conversations(id,title) VALUES('shared:id','fresh title')")
        self.db.execute("INSERT INTO conversation_messages(id,conversation_id,role,content) VALUES('shared:id','shared:id','user','fresh message')")
        self.db.execute("INSERT INTO conversation_message_bookmarks(id,conversation_id,message_id,title) VALUES('shared:id','shared:id','shared:id','fresh bookmark')")
        self.assertEqual(self.matches("conversation_search_fts", "fresh"), 3)
        self.assertEqual(self.db.execute("SELECT count(*) FROM conversation_search_fts_rowids WHERE source_id='shared:id'").fetchone()[0], 3)
        self.db.execute("UPDATE conversation_message_bookmarks SET id='renamed-bookmark' WHERE id='shared:id'")
        self.assertEqual(self.db.execute("SELECT source_id FROM conversation_search_fts_rowids WHERE source='bookmark' AND source_id='renamed-bookmark'").fetchone(), ("renamed-bookmark",))
        self.db.execute("DELETE FROM conversations WHERE id='shared:id'")
        self.assertEqual(self.matches("conversation_search_fts", "fresh"), 0)
        self.assertEqual(self.db.execute("SELECT count(*) FROM conversation_search_fts_rowids WHERE source_id='shared:id'").fetchone()[0], 0)
        self.assertIsNone(self.db.execute("SELECT 1 FROM conversation_search_fts_rowids WHERE source_id='renamed-bookmark'").fetchone())
        self.assertEqual(self.matches("conversation_search_fts", "original"), 3)

    def test_rollback_clear_and_rebuild_preserve_mapping(self):
        rowid = self.db.execute("SELECT fts_rowid FROM chunk_fts_rowids WHERE chunk_id='c1'").fetchone()[0]
        self.db.execute("BEGIN")
        self.db.execute("UPDATE text_chunks SET content='temporary' WHERE id='c1'")
        self.db.rollback()
        self.assertEqual(self.db.execute("SELECT content FROM chunks_fts WHERE rowid=?", (rowid,)).fetchone()[0], "alpha original")
        self.db.execute("DELETE FROM chunks_fts")
        self.db.execute("DELETE FROM chunks_trigram")
        self.db.execute("UPDATE text_chunks SET content='after clear' WHERE id='c1'")
        self.assertEqual(self.matches("chunks_fts", "clear"), 1)
        self.assertEqual(self.db.execute("SELECT count(*) FROM chunks_fts WHERE chunk_id='c1'").fetchone()[0], 1)
        self.db.execute("INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild')")
        self.db.execute("INSERT INTO chunks_trigram(chunks_trigram) VALUES('rebuild')")
        self.db.execute("DELETE FROM text_chunks WHERE id='c1'")
        self.assertEqual(self.matches("chunks_fts", "clear"), 0)

    def test_chunk_update_uses_indexed_identity_lookup(self):
        plan = self.db.execute("EXPLAIN QUERY PLAN SELECT fts_rowid FROM chunk_fts_rowids WHERE chunk_id='c1'").fetchall()
        self.assertTrue(any("INDEX" in str(row).upper() or "PRIMARY KEY" in str(row).upper() for row in plan), plan)

    def test_trigger_work_stays_bounded_as_corpus_grows(self):
        def scaled_database(size):
            db = sqlite3.connect(":memory:")
            db.execute("PRAGMA foreign_keys=ON")
            for migration in sorted(p for p in MIGRATIONS.glob("*.sql") if p.name < TARGET.name):
                db.executescript("BEGIN;\n" + migration.read_text() + "\nCOMMIT;")
            db.execute("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum) VALUES('d','/d','d',1,'now','x')")
            db.execute("INSERT INTO conversations(id,title) VALUES('v','scale')")
            db.executemany(
                "INSERT INTO text_chunks(id,document_id,content,context_prefix,chunk_index) VALUES(?, 'd', ?, 'H', ?)",
                ((f'c{i}', f'chunk body {i}', i) for i in range(size)),
            )
            db.executemany(
                "INSERT INTO conversation_messages(id,conversation_id,role,content) VALUES(?, 'v', 'user', ?)",
                ((f'm{i}', f'message body {i}') for i in range(size)),
            )
            db.executemany(
                "INSERT INTO embedding_generation_vectors(model_identity,chunk_id,content_hash,embedding,dimension) VALUES('model',?,'hash',X'00000000',1)",
                ((f'c{i}',) for i in range(size)),
            )
            db.commit()
            for migration in sorted(p for p in MIGRATIONS.glob("*.sql") if p.name >= TARGET.name):
                db.executescript("BEGIN;\n" + migration.read_text() + "\nCOMMIT;")
            return db

        def measured_steps(db, statement, args=()):
            steps = [0]
            db.set_progress_handler(lambda: steps.__setitem__(0, steps[0] + 100) or 0, 100)
            try:
                db.execute(statement, args)
                db.commit()
            finally:
                db.set_progress_handler(None, 0)
            return steps[0]

        small, large = scaled_database(500), scaled_database(5000)
        try:
            small_delete = measured_steps(small, "DELETE FROM text_chunks WHERE id IN (SELECT id FROM text_chunks ORDER BY id LIMIT 20)")
            large_delete = measured_steps(large, "DELETE FROM text_chunks WHERE id IN (SELECT id FROM text_chunks ORDER BY id LIMIT 20)")
            small_update = measured_steps(small, "UPDATE conversation_messages SET content=content||' revised' WHERE id IN (SELECT id FROM conversation_messages ORDER BY id LIMIT 20)")
            large_update = measured_steps(large, "UPDATE conversation_messages SET content=content||' revised' WHERE id IN (SELECT id FROM conversation_messages ORDER BY id LIMIT 20)")
            self.assertLess(large_delete, small_delete * 3 + 100, (small_delete, large_delete))
            self.assertLess(large_update, small_update * 3 + 100, (small_update, large_update))
        finally:
            small.close()
            large.close()


if __name__ == "__main__":
    unittest.main()
