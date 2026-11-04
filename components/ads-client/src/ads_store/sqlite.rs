use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use sql_support::{open_database, LazyDb};

use crate::{
    ads::{Ad, PlacementId},
    ads_store::{connection_initializer::AdsStoreConnectionInitializer, store::AdsStoreDb},
    bytesize::ByteSize,
    clock::Clock,
    mars::error::FetchAdsError,
};

impl AdsStoreDb for LazyDb<AdsStoreConnectionInitializer> {
    /// Removes all entries from cache.
    fn clear_all(&self) -> Result<usize, open_database::Error> {
        let (conn, _) = self.lock()?;
        let mut total = 0;
        total += conn.execute("DELETE FROM ads", [])?;
        Ok(total)
    }

    fn close(&self) {
        self.close(true);
    }

    /// Returns whether the db is in memory rather than backed by a file.
    fn is_memory(&self) -> Result<bool, open_database::Error> {
        let (conn, _) = self.lock()?;
        // SQLite reports an empty path for in-memory dbs.
        Ok(conn.path() == Some(""))
    }

    /// Returns total size of the cache in bytes.
    #[cfg(test)]
    fn current_total_size_bytes(&self) -> Result<ByteSize, open_database::Error> {
        let (conn, _) = self.lock()?;
        let size_bytes_ads: u64 =
            conn.query_row("SELECT COALESCE(SUM(size_bytes),0) FROM ads", [], |row| {
                row.get(0)
            })?;
        Ok(ByteSize::b(size_bytes_ads))
    }

    fn delete_expired_entries(&self, clock: Arc<dyn Clock>) -> Result<usize, open_database::Error> {
        let (conn, _) = self.lock()?;
        Ok(conn.execute(
            "DELETE FROM ads_store WHERE expires_at <= ?1",
            params![clock.now_epoch_seconds()],
        )?)
    }

    fn invalidate_ad_by_id(
        &self,
        placement_id: &PlacementId,
    ) -> Result<usize, open_database::Error> {
        let (conn, _) = self.lock()?;
        Ok(conn.execute(
            "DELETE FROM ads WHERE placement_id = ?1",
            params![&placement_id.as_ref()],
        )?)
    }

    fn lookup(&self, placement_id: &PlacementId) -> Result<Option<Vec<Ad>>, FetchAdsError> {
        let (conn, _) = self.lock()?;
        let res = conn
            .query_row(
                "SELECT placement_id, ad_body
                 FROM ads
                 WHERE placement_id = ?1",
                params![placement_id.as_ref()],
                |row| {
                    let ad_body: Vec<u8> = row.get(1)?;
                    Ok(ad_body)
                },
            )
            .optional()
            .map_err(open_database::Error::from)?;
        Ok(res.map(|x| serde_json::from_slice(&x)).transpose()?)
    }

    /// Upsert the ads for a placement, replacing any ads already stored for it.
    fn store_ad(
        &self,
        placement_id: &PlacementId,
        ads: &[Ad],
        clock: Arc<dyn Clock>,
    ) -> Result<(), FetchAdsError> {
        let placement_id_str: &str = placement_id.as_ref();
        let ad_body = serde_json::to_vec(&ads)?;
        let size_bytes = ad_body.len() as i64;
        let now = clock.now_epoch_seconds();

        let (conn, _) = self.lock()?;
        conn.execute(
            "INSERT INTO ads (
                stored_at,
                placement_id,
                ad_body,
                size_bytes
            )
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(placement_id) DO UPDATE SET
                stored_at=excluded.stored_at,
                ad_body=excluded.ad_body,
                size_bytes=excluded.size_bytes",
            params![now, placement_id_str, ad_body, size_bytes,],
        )
        .map_err(open_database::Error::from)?;
        Ok(())
    }

    fn trim_to_max_size(&self, max_size: &ByteSize) -> Result<(), open_database::Error> {
        // Keeps the newest rows that fit in `max_size` and deletes the rest,
        // in one statement.
        let (conn, _) = self.lock()?;
        conn.execute(
            "DELETE FROM ads
             WHERE rowid IN (
                 SELECT rowid
                 FROM (
                     SELECT rowid,
                            SUM(size_bytes) OVER (ORDER BY stored_at DESC, rowid DESC) AS kept_bytes
                     FROM ads
                 )
                 WHERE kept_bytes > ?1
             )",
            params![max_size.as_u64()],
        )?;
        Ok(())
    }
}
