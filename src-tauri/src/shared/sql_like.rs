//! Escaping helpers for SQL `LIKE` patterns.
//!
//! Parameter binding stops SQL *injection*, but it does not stop a bound value
//! from being interpreted as a `LIKE` pattern. `%` still matches any run of
//! characters and `_` still matches any single character once the value is on
//! the right-hand side of `LIKE`. A user folder called `my_notes` therefore
//! matches `myXnotes`, and a prefix match on `/data/notes` also matches
//! `/data/notes-archive`.
//!
//! Every `LIKE` whose pattern contains user- or filesystem-derived text must
//! route through here and pair the query with `ESCAPE '\'`.

/// The escape character these helpers emit. Callers must append
/// `ESCAPE '\'` to any `LIKE` clause using a pattern from this module.
pub const LIKE_ESCAPE_CHAR: char = '\\';

/// Escapes `LIKE` metacharacters in `input` so it matches literally.
///
/// The backslash must be escaped first, otherwise the backslashes introduced
/// when escaping `%` and `_` would themselves be doubled.
pub fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push(LIKE_ESCAPE_CHAR);
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Builds a `LIKE` pattern matching `input` anywhere in a column
/// (`%needle%`), with metacharacters in `input` escaped.
pub fn contains_pattern(input: &str) -> String {
    format!("%{}%", escape_like(input))
}

/// Builds a `LIKE` pattern matching every path **strictly inside** the
/// directory `dir`.
///
/// Two things matter here and both have bitten this codebase:
///
/// 1. A trailing separator is required. Without it, `/data/notes` also matches
///    `/data/notes-archive`, `/data/notes.bak`, and any other sibling sharing
///    the prefix — deleting documents the user never asked to remove.
/// 2. The path is escaped, so a directory literally named `my_notes` does not
///    also match `myXnotes`.
///
/// Note this deliberately excludes a row whose path *is* `dir` exactly;
/// callers that also want the directory's own row should match it separately
/// with `=` rather than widening the pattern.
pub fn directory_prefix_pattern(dir: &str) -> String {
    let mut prefix = dir.to_string();
    if !prefix.ends_with(std::path::MAIN_SEPARATOR) {
        prefix.push(std::path::MAIN_SEPARATOR);
    }
    format!("{}%", escape_like(&prefix))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Row;

    #[test]
    fn escapes_each_metacharacter() {
        assert_eq!(escape_like("my_notes"), r"my\_notes");
        assert_eq!(escape_like("100%"), r"100\%");
        assert_eq!(escape_like(r"a\b"), r"a\\b");
        assert_eq!(escape_like("plain"), "plain");
    }

    #[test]
    fn escapes_backslash_before_other_metacharacters() {
        // If `%` were escaped first, the inserted backslash would then be
        // doubled by the backslash pass, producing `\\%` — a literal backslash
        // followed by a wildcard.
        assert_eq!(escape_like(r"50\%"), r"50\\\%");
    }

    #[test]
    fn directory_pattern_appends_separator() {
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            directory_prefix_pattern("/data/notes"),
            format!("/data/notes{}%", sep)
        );
        // Already-terminated paths are not double-separated.
        assert_eq!(
            directory_prefix_pattern(&format!("/data/notes{}", sep)),
            format!("/data/notes{}%", sep)
        );
    }

    /// The behaviour that matters is what SQLite actually matches, so assert
    /// against a real database rather than on the pattern string alone.
    #[tokio::test]
    async fn sqlite_matches_only_the_intended_subtree() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("open in-memory db");

        sqlx::query("CREATE TABLE docs (file_path TEXT NOT NULL)")
            .execute(&pool)
            .await
            .expect("create table");

        let sep = std::path::MAIN_SEPARATOR;
        let paths = [
            format!("/data/notes{}a.md", sep),           // inside — match
            format!("/data/notes{}sub{}b.md", sep, sep), // nested — match
            format!("/data/notes-archive{}c.md", sep),   // sibling — must NOT match
            format!("/data/notes.bak{}d.md", sep),       // sibling — must NOT match
            "/data/notes".to_string(),                   // the dir row itself — excluded
            format!("/data/other{}e.md", sep),           // unrelated — must NOT match
        ];
        for p in &paths {
            sqlx::query("INSERT INTO docs (file_path) VALUES (?)")
                .bind(p)
                .execute(&pool)
                .await
                .expect("insert");
        }

        let rows = sqlx::query(
            r"SELECT file_path FROM docs WHERE file_path LIKE ? ESCAPE '\' ORDER BY file_path",
        )
        .bind(directory_prefix_pattern("/data/notes"))
        .fetch_all(&pool)
        .await
        .expect("select");

        let matched: Vec<String> = rows.iter().map(|r| r.get::<String, _>(0)).collect();
        assert_eq!(
            matched,
            vec![
                format!("/data/notes{}a.md", sep),
                format!("/data/notes{}sub{}b.md", sep, sep),
            ],
            "only paths strictly inside /data/notes should match"
        );
    }

    /// A directory whose name contains `_` must not match arbitrary characters
    /// in that position.
    #[tokio::test]
    async fn sqlite_treats_underscore_literally() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("open in-memory db");

        sqlx::query("CREATE TABLE docs (file_path TEXT NOT NULL)")
            .execute(&pool)
            .await
            .expect("create table");

        let sep = std::path::MAIN_SEPARATOR;
        for p in [
            format!("/data/my_notes{}a.md", sep),
            format!("/data/myXnotes{}b.md", sep),
        ] {
            sqlx::query("INSERT INTO docs (file_path) VALUES (?)")
                .bind(&p)
                .execute(&pool)
                .await
                .expect("insert");
        }

        let count: i64 =
            sqlx::query_scalar(r"SELECT COUNT(*) FROM docs WHERE file_path LIKE ? ESCAPE '\'")
                .bind(directory_prefix_pattern("/data/my_notes"))
                .fetch_one(&pool)
                .await
                .expect("count");

        assert_eq!(count, 1, "`_` must not act as a single-character wildcard");
    }
}
