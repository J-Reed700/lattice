import sqlite3
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MIGRATIONS = ROOT / "crates/lattice-desktop/migrations"


def new_db():
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys = ON")
    return db


def apply_migrations(db, stop_before=None):
    for migration in sorted(MIGRATIONS.glob("*.sql")):
        if stop_before is not None and migration.name >= stop_before:
            break
        db.executescript("BEGIN;\n" + migration.read_text() + "\nCOMMIT;")


class PersistenceInvariantTests(unittest.TestCase):
    def setUp(self):
        self.db = new_db()
        apply_migrations(self.db)

    def tearDown(self):
        self.db.close()

    def test_full_chain_upgrade_preserves_linked_rows_and_constraints(self):
        db = new_db()
        target = "20261025121000_persistence_invariants.sql"
        apply_migrations(db, stop_before=target)
        db.execute(
            "INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum,status) "
            "VALUES('keep','/A.txt','A.txt',4,'now','hash','indexed')"
        )
        db.execute("INSERT INTO image_metadata(id,document_id) VALUES('image','keep')")
        db.execute("INSERT INTO text_chunks(id,document_id,content,chunk_index) VALUES('chunk','keep','body',0)")
        db.execute("INSERT INTO embedding_generation_vectors(model_identity,chunk_id,content_hash,embedding,dimension) VALUES('model','chunk','hash',X'00',1)")
        db.execute("INSERT INTO conversations(id,title) VALUES('conversation','title')")
        db.execute("INSERT INTO conversation_messages(id,conversation_id,role,content) VALUES('message','conversation','user','body')")
        db.execute("INSERT INTO conversation_summaries(id,conversation_id,summary_text,up_to_message_id,original_message_count,original_tokens,summary_tokens,compression_ratio) VALUES('summary','conversation','summary','message',1,2,1,0.5)")
        db.commit()
        db.executescript("BEGIN;\n" + (MIGRATIONS / target).read_text() + "\nCOMMIT;")
        row = db.execute("SELECT id,file_path,size_bytes,status FROM documents").fetchone()
        self.assertEqual(row, ("keep", "/A.txt", 4, "indexed"))
        self.assertEqual(db.execute("SELECT COUNT(*) FROM image_metadata WHERE id='image'").fetchone()[0], 1)
        self.assertEqual(db.execute("SELECT COUNT(*) FROM embedding_generation_vectors WHERE chunk_id='chunk'").fetchone()[0], 1)
        self.assertEqual(db.execute("SELECT COUNT(*) FROM conversation_summaries WHERE id='summary'").fetchone()[0], 1)
        self.assertTrue(db.execute(
            "SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_documents_file_path'"
        ).fetchone())
        self.assertEqual(db.execute("SELECT id FROM documents WHERE file_path='/a.TXT' COLLATE NOCASE").fetchone(), ("keep",))
        db.execute("DELETE FROM documents WHERE id='keep'")
        self.assertEqual(db.execute("SELECT COUNT(*) FROM image_metadata WHERE id='image'").fetchone()[0], 0)
        self.assertEqual(db.execute("SELECT COUNT(*) FROM embedding_generation_vectors WHERE chunk_id='chunk'").fetchone()[0], 0)
        db.close()

    def test_invalid_existing_document_rolls_back_upgrade_without_data_loss(self):
        db = new_db()
        target = "20261025121000_persistence_invariants.sql"
        apply_migrations(db, stop_before=target)
        db.execute("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum,status) VALUES('bad','/bad','bad',-1,'now','h','pending')")
        db.commit()
        with self.assertRaisesRegex(sqlite3.IntegrityError, "documents_id_size_status_invariant"):
            db.executescript("BEGIN;\n" + (MIGRATIONS / target).read_text() + "\nCOMMIT;")
        # executescript leaves the failed transaction open; roll it back just as
        # the migration runner does when a transactional upgrade fails.
        db.rollback()
        self.assertEqual(db.execute("SELECT size_bytes FROM documents WHERE id='bad'").fetchone(), (-1,))
        self.assertTrue(db.execute("SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_tags_name'").fetchone())
        self.assertIsNone(db.execute("SELECT 1 FROM sqlite_master WHERE type='trigger' AND name='documents_invariants_before_insert'").fetchone())
        db.close()

    def test_document_invariants_reject_bad_insert_and_update(self):
        values = ("d", "/d", "d", 0, "now", "hash", "pending")
        self.db.execute(
            "INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum,status) "
            "VALUES(?,?,?,?,?,?,?)", values,
        )
        for sql, args in (
            ("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum,status) VALUES(NULL,'/null','n',0,'now','h','pending')", ()),
            ("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum,status) VALUES('neg','/neg','n',-1,'now','h','pending')", ()),
            ("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum,status) VALUES('text','/text','n','abc','now','h','pending')", ()),
            ("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum,status) VALUES('real','/real','n',1.5,'now','h','pending')", ()),
            ("INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum,status) VALUES('bad','/bad','n',0,'now','h','unknown')", ()),
            ("UPDATE documents SET size_bytes=-2 WHERE id='d'", ()),
            ("UPDATE documents SET size_bytes='abc' WHERE id='d'", ()),
            ("UPDATE documents SET status='unknown' WHERE id='d'", ()),
            ("UPDATE documents SET id=NULL WHERE id='d'", ()),
        ):
            with self.subTest(sql=sql), self.assertRaises(sqlite3.IntegrityError):
                self.db.execute(sql, args)
        self.assertEqual(self.db.execute("SELECT id,size_bytes,status FROM documents WHERE id='d'").fetchone(), ("d", 0, "pending"))

    def test_supported_statuses_and_fk_child_indexes(self):
        for i, status in enumerate(("pending", "processing", "indexed", "failed")):
            self.db.execute(
                "INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum,status) "
                "VALUES(?,?,?,?,?,?,?)", (str(i), f"/{i}", "n", 0, "now", "h", status),
            )
        for table, column, index in (
            ("embedding_generation_vectors", "chunk_id", "idx_embedding_generation_vectors_chunk_id"),
            ("image_metadata", "document_id", "idx_image_metadata_document_id"),
            ("conversation_summaries", "up_to_message_id", "idx_conversation_summaries_up_to_message_id"),
        ):
            found = self.db.execute(
                "SELECT 1 FROM sqlite_master WHERE type='index' AND name=?", (index,)
            ).fetchone()
            self.assertIsNotNone(found)
            plan = self.db.execute(f"EXPLAIN QUERY PLAN SELECT rowid FROM {table} WHERE {column}=?", ("x",)).fetchall()
            self.assertTrue(any(index in str(row) for row in plan), plan)

        # The child indexes stay usable while cascades remove dependent rows.
        self.db.execute(
            "INSERT INTO documents(id,file_path,file_name,size_bytes,modified_at,checksum) "
            "VALUES('cascade','/cascade','cascade',0,'now','h')"
        )
        self.db.execute(
            "INSERT INTO image_metadata(id,document_id) VALUES('image','cascade')"
        )
        self.db.execute(
            "INSERT INTO text_chunks(id,document_id,content,chunk_index) "
            "VALUES('chunk','cascade','body',0)"
        )
        self.db.execute(
            "INSERT INTO embedding_generation_vectors(model_identity,chunk_id,content_hash,embedding,dimension) "
            "VALUES('model','chunk','hash',X'00',1)"
        )
        self.db.execute("DELETE FROM documents WHERE id='cascade'")
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM image_metadata WHERE id='image'").fetchone()[0], 0)
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM embedding_generation_vectors WHERE chunk_id='chunk'").fetchone()[0], 0)

    def test_unique_constraints_remain_after_redundant_index_removal(self):
        expected_gone = (
            "idx_conversation_summaries_conversation_id", "idx_document_space_memberships_document_space",
            "idx_document_tags_composite", "idx_favorites_document_id", "idx_mentions_name",
            "idx_models_model_id", "idx_recent_documents_document_id", "idx_tags_name", "idx_watch_folders_path",
        )
        for index in expected_gone:
            self.assertIsNone(self.db.execute(
                "SELECT 1 FROM sqlite_master WHERE type='index' AND name=?", (index,)
            ).fetchone())
        self.db.execute("INSERT INTO tags(id,name) VALUES('t1','same')")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO tags(id,name) VALUES('t2','SAME')")


if __name__ == "__main__":
    unittest.main()
