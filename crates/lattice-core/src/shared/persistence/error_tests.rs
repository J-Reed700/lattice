use super::{PersistenceError, PersistenceErrorKind};
use crate::shared::error::AppError;

fn kind_of(error: sqlx::Error) -> PersistenceErrorKind {
    PersistenceError::from(error).kind()
}

#[test]
fn each_driver_failure_has_its_kind() {
    assert_eq!(
        kind_of(sqlx::Error::RowNotFound),
        PersistenceErrorKind::NotFound
    );
    assert_eq!(
        kind_of(sqlx::Error::PoolTimedOut),
        PersistenceErrorKind::Busy
    );
    assert_eq!(
        kind_of(sqlx::Error::PoolClosed),
        PersistenceErrorKind::Closed
    );
    assert_eq!(
        kind_of(sqlx::Error::Configuration("bad".into())),
        PersistenceErrorKind::Misconfigured
    );
    assert_eq!(
        kind_of(sqlx::Error::WorkerCrashed),
        PersistenceErrorKind::Other
    );
}

#[test]
fn the_message_is_the_drivers() {
    let driver = sqlx::Error::PoolTimedOut;
    let expected = driver.to_string();
    assert_eq!(PersistenceError::from(driver).to_string(), expected);
}

#[test]
fn an_app_error_carries_the_drivers_message() {
    let expected = sqlx::Error::RowNotFound.to_string();
    let app: AppError = PersistenceError::from(sqlx::Error::RowNotFound).into();
    assert!(matches!(app, AppError::Database(message) if message == expected));
}

#[test]
fn the_driver_error_comes_back_for_a_data_function() {
    let error: sqlx::Error = PersistenceError::from(sqlx::Error::PoolClosed).into();
    assert!(matches!(error, sqlx::Error::PoolClosed));
}

#[test]
fn a_busy_driver_is_a_busy_database_with_the_drivers_message() {
    let expected = sqlx::Error::PoolTimedOut.to_string();
    assert!(matches!(
        AppError::from(sqlx::Error::PoolTimedOut),
        AppError::DatabaseBusy(message) if message == expected
    ));
}
