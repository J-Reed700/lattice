-- The digest of a file Lattice has started writing. A write that lands
-- before its baseline is recorded is still recognised as Lattice's own.
ALTER TABLE vault_mirror_state ADD COLUMN writing_sha256 TEXT;
