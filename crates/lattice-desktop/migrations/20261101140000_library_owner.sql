-- Which database owns the files under the lattice home. The startup sweeps
-- delete files no row here names, so a database that did not import them must
-- not run them: a fresh one made when a corrupt database was moved aside, or
-- the one of a second copy of the app. The same id is kept in
-- `<lattice home>/owner`, and the sweeps run only where the two agree. One row,
-- written by the app when this database first takes the files.
CREATE TABLE library_owner (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    owner_id TEXT NOT NULL
) STRICT;
