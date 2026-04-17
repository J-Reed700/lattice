//! # Favorites feature
//!
//! User-starred documents. Add / remove / list / check.
//!
//! ## File layout
//!
//! | File             | Canonical module path                                             |
//! |------------------|-------------------------------------------------------------------|
//! | `dto.rs`         | `crate::application::dtos::favorite_dto`                          |
//! | `mapper.rs`      | `crate::application::mappers::favorite_mapper`                    |
//! | `use_cases/`     | `crate::application::use_cases::favorites`                        |
//! | `repository.rs`  | `crate::infrastructure::persistence::repositories::favorites_repository` |
//! | `commands.rs`    | `crate::interfaces::commands::favorites` (aka `favorites_commands`) |
//! | `plugin.rs`      | `crate::plugins::favorites_plugin`                                |
//!
//! `FavoritesRepositoryPort` stays in `application/ports/`.
