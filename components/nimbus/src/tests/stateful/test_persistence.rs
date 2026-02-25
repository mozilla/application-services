/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::fs;

use rkv::StoreOptions;

use crate::error::Result;
use crate::stateful::enrollment::{get_experiment_participation, get_rollout_participation};
use crate::stateful::persistence::*;

#[test]
fn test_db_upgrade_no_version() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;

    let rkv = Database::open_rkv(&tmp_dir)?;
    let _meta_store = rkv.open_single("meta", StoreOptions::create())?;
    let experiment_store =
        SingleStore::new(rkv.open_single("experiments", StoreOptions::create())?);
    let enrollment_store =
        SingleStore::new(rkv.open_single("enrollments", StoreOptions::create())?);
    let mut writer = rkv.write()?;
    enrollment_store.put(&mut writer, "foo", &"bar".to_owned())?;
    experiment_store.put(&mut writer, "bobo", &"tron".to_owned())?;
    writer.commit()?;

    let db = Database::new(&tmp_dir)?;
    assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(DB_VERSION));
    assert!(db.collect_all::<String>(StoreId::Enrollments)?.is_empty());
    assert!(db.collect_all::<String>(StoreId::Experiments)?.is_empty());

    Ok(())
}

#[test]
fn test_db_upgrade_unknown_version() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;

    let rkv = Database::open_rkv(&tmp_dir)?;
    let meta_store = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);
    let experiment_store =
        SingleStore::new(rkv.open_single("experiments", StoreOptions::create())?);
    let enrollment_store =
        SingleStore::new(rkv.open_single("enrollments", StoreOptions::create())?);
    let mut writer = rkv.write()?;
    meta_store.put(&mut writer, DB_KEY_DB_VERSION, &u16::MAX)?;
    enrollment_store.put(&mut writer, "foo", &"bar".to_owned())?;
    experiment_store.put(&mut writer, "bobo", &"tron".to_owned())?;
    writer.commit()?;
    let db = Database::new(&tmp_dir)?;
    assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(DB_VERSION));
    assert!(db.collect_all::<String>(StoreId::Enrollments)?.is_empty());
    assert!(db.collect_all::<String>(StoreId::Experiments)?.is_empty());

    Ok(())
}

#[test]
fn test_corrupt_db() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;

    let db_dir = tmp_dir.path().join("db");
    fs::create_dir(db_dir.clone())?;

    // The database filename differs depending on the rkv mode.
    #[cfg(feature = "rkv-safe-mode")]
    let db_file = db_dir.join("data.safe.bin");
    #[cfg(not(feature = "rkv-safe-mode"))]
    let db_file = db_dir.join("data.mdb");

    let garbage = b"Not a database!";
    let garbage_len = garbage.len() as u64;
    fs::write(&db_file, garbage)?;
    assert_eq!(fs::metadata(&db_file)?.len(), garbage_len);
    // Opening the DB should delete the corrupt file and replace it.
    Database::new(&tmp_dir)?;
    // Old contents should be removed and replaced with actual data.
    assert_ne!(fs::metadata(&db_file)?.len(), garbage_len);
    Ok(())
}

#[test]
fn test_migrate_db_v2_to_v3_user_opted_out() -> Result<()> {
    error_support::init_for_tests();
    let tmp_dir = tempfile::tempdir()?;

    // Create a v2 database where user opted out globally
    create_old_database_v2_with_global_participation(&tmp_dir, false)?;

    // Open with new version - should trigger migration
    let db = Database::new(&tmp_dir)?;

    // Check the database was upgraded to v3
    assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(3u16));

    // Check that separate flags were set correctly for opted-out user
    let reader = db.read()?;
    assert!(
        !get_experiment_participation(&db, &reader)?, // Should preserve opt-out choice for experiments
    );
    assert!(
        !get_rollout_participation(&db, &reader)?, // Should preserve opt-out choice for rollouts
    );

    // Check old key was removed
    assert_eq!(
        db.get::<bool>(StoreId::Meta, DB_KEY_GLOBAL_USER_PARTICIPATION)?,
        None
    );

    Ok(())
}

#[test]
fn test_migrate_db_v2_to_v3_user_opted_in() -> Result<()> {
    error_support::init_for_tests();
    let tmp_dir = tempfile::tempdir()?;

    // Create a v2 database where user was opted in globally
    create_old_database_v2_with_global_participation(&tmp_dir, true)?;

    let db = Database::new(&tmp_dir)?;

    // Check the database was upgraded to v3
    assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(3u16));

    // Check that separate flags were set correctly for opted-in user
    let reader = db.read()?;
    assert!(
        get_experiment_participation(&db, &reader)?, // Should preserve opt-in choice for experiments
    );
    assert!(
        get_rollout_participation(&db, &reader)?, // Should preserve opt-in choice for rollouts
    );

    // Check old key was removed
    assert_eq!(
        db.get::<bool>(StoreId::Meta, DB_KEY_GLOBAL_USER_PARTICIPATION)?,
        None
    );

    Ok(())
}

#[test]
fn test_migrate_empty() -> Result<()> {
    error_support::init_for_tests();

    let tmp_dir = tempfile::tempdir()?;

    let db = Database::new(&tmp_dir)?;
    let meta = db.get_store(StoreId::Meta);
    let reader = db.read()?;
    assert_eq!(meta.get::<u16, _>(&reader, DB_KEY_DB_VERSION)?, Some(3));
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_GLOBAL_USER_PARTICIPATION)?,
        None
    );
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_EXPERIMENT_PARTICIPATION)?,
        Some(true)
    );
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_ROLLOUT_PARTICIPATION)?,
        Some(true)
    );

    Ok(())
}

#[test]
fn test_migrate_db_v1_to_v3_cumulative_participation_enabled() -> Result<()> {
    error_support::init_for_tests();

    let tmp_dir = tempfile::tempdir()?;

    {
        let rkv = Database::open_rkv(&tmp_dir)?;
        let meta = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);

        let mut writer = rkv.write()?;
        meta.put(&mut writer, DB_KEY_DB_VERSION, &1)?;
        meta.put(&mut writer, DB_KEY_GLOBAL_USER_PARTICIPATION, &true)?;
        writer.commit()?;
    }

    // Open the database and migrate it.
    let db = Database::new(&tmp_dir)?;
    let meta = db.get_store(StoreId::Meta);
    let reader = db.read()?;
    assert_eq!(meta.get::<u16, _>(&reader, DB_KEY_DB_VERSION)?, Some(3));
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_GLOBAL_USER_PARTICIPATION)?,
        None
    );
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_EXPERIMENT_PARTICIPATION)?,
        Some(true)
    );
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_ROLLOUT_PARTICIPATION)?,
        Some(true)
    );

    Ok(())
}

#[test]
fn test_migrate_db_v1_to_v3_cumulative_participation_disabled() -> Result<()> {
    error_support::init_for_tests();

    let tmp_dir = tempfile::tempdir()?;

    {
        let rkv = Database::open_rkv(&tmp_dir)?;
        let meta = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);

        let mut writer = rkv.write()?;
        meta.put(&mut writer, DB_KEY_DB_VERSION, &1)?;
        meta.put(&mut writer, DB_KEY_GLOBAL_USER_PARTICIPATION, &false)?;
        writer.commit()?;
    }

    // Open the database and migrate it.
    let db = Database::new(&tmp_dir)?;
    let meta = db.get_store(StoreId::Meta);
    let reader = db.read()?;
    assert_eq!(meta.get::<u16, _>(&reader, DB_KEY_DB_VERSION)?, Some(3));
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_GLOBAL_USER_PARTICIPATION)?,
        None
    );
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_EXPERIMENT_PARTICIPATION)?,
        Some(false)
    );
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_ROLLOUT_PARTICIPATION)?,
        Some(false)
    );

    Ok(())
}

#[test]
fn test_migrate_db_v3_idempotent() -> Result<()> {
    error_support::init_for_tests();

    let tmp_dir = tempfile::tempdir()?;

    {
        let rkv = Database::open_rkv(&tmp_dir)?;
        let meta = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);

        let mut writer = rkv.write()?;
        meta.put(&mut writer, DB_KEY_DB_VERSION, &3)?;
        meta.put(&mut writer, DB_KEY_EXPERIMENT_PARTICIPATION, &false)?;
        meta.put(&mut writer, DB_KEY_ROLLOUT_PARTICIPATION, &true)?;
        writer.commit()?;
    }

    // Open the database and migrate it. The fields should be unchanged.
    let db = Database::new(&tmp_dir)?;
    let meta = db.get_store(StoreId::Meta);
    let reader = db.read()?;
    assert_eq!(meta.get::<u16, _>(&reader, DB_KEY_DB_VERSION)?, Some(3));
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_GLOBAL_USER_PARTICIPATION)?,
        None
    );
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_EXPERIMENT_PARTICIPATION)?,
        Some(false)
    );
    assert_eq!(
        meta.get::<bool, _>(&reader, DB_KEY_ROLLOUT_PARTICIPATION)?,
        Some(true)
    );

    Ok(())
}

// Helper function to create a v2 database with global participation flag
fn create_old_database_v2_with_global_participation(
    tmp_dir: &tempfile::TempDir,
    global_participation: bool,
) -> Result<()> {
    let rkv = Database::open_rkv(tmp_dir)?;
    let meta_store = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);
    let mut writer = rkv.write()?;

    // Set version to 2
    meta_store.put(&mut writer, DB_KEY_DB_VERSION, &2u16)?;

    // Set global participation flag (the old way)
    meta_store.put(
        &mut writer,
        DB_KEY_GLOBAL_USER_PARTICIPATION,
        &global_participation,
    )?;

    writer.commit()?;
    Ok(())
}
