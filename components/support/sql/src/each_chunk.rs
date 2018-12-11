/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::{self, limits::Limit, types::ToSql};

/// Returns SQLITE_LIMIT_VARIABLE_NUMBER as read from an in-memory connection and cached.
/// connection and cached. That means this will return the wrong value if it's set to a lower
/// value for a connection using this will return the wrong thing, but doing so is rare enough
/// that we explicitly don't support it (why would you want to lower this at runtime?).
///
/// If you call this and the actual value was set to a negative number or zero (nothing prevents
/// this beyond a warning in the SQLite documentation), we panic. However, it's unlikely you can
/// run useful queries if this happened anyway.
pub fn default_max_variable_number() -> usize {
    lazy_static! {
        static ref MAX_VARIABLE_NUMBER: usize = {
            let conn = rusqlite::Connection::open_in_memory()
                .expect("Failed to initialize in-memory connection (out of memory?)");

            let limit = conn.limit(Limit::SQLITE_LIMIT_VARIABLE_NUMBER);
            assert!(
                limit > 0,
                "Illegal value for SQLITE_LIMIT_VARIABLE_NUMBER (must be > 0) {}",
                limit
            );
            limit as usize
        };
    }
    *MAX_VARIABLE_NUMBER
}

/// Helper for the case where you have a `&[impl ToSql]` of arbitrary length, but need a
/// `&[&dyn ToSql]` of no more than the connection's `MAX_VARIABLE_NUMBER` (rather,
/// `default_max_variable_number()`). This is useful when performing batched updates.
///
/// The `do_chunk` callback is called with a slice of no more than `default_max_variable_number()`
/// items as it's first argument, and the offset from the start as it's second.
///
/// See `each_chunk_mapped` for the case where `T` doesn't implement `ToSql`, but converting to
/// `&dyn ToSql` is nonetheless possible.
pub fn each_chunk<'a, T, E, F>(items: &[T], do_chunk: F) -> Result<(), E>
where
    T: ToSql + 'a,
    F: FnMut(&[&dyn ToSql], usize) -> Result<(), E>,
{
    each_sized_chunk_mapped(
        items,
        default_max_variable_number(),
        |t| t as &dyn ToSql,
        do_chunk,
    )
}

/// A version of `each_chunk` for the case when the conversion to `to_sql` requires an custom
/// intermediate step. For example, you might want to grab a property off of an arrray of records
pub fn each_chunk_mapped<'a, T, E, Mapper, DoChunk>(
    items: &'a [T],
    to_sql: Mapper,
    do_chunk: DoChunk,
) -> Result<(), E>
where
    T: 'a,
    Mapper: Fn(&'a T) -> &'a dyn ToSql,
    DoChunk: FnMut(&[&dyn ToSql], usize) -> Result<(), E>,
{
    each_sized_chunk_mapped(items, default_max_variable_number(), to_sql, do_chunk)
}

/// Utility to help perform batched updates, inserts, queries, etc. This is the low-level version
/// of this utility which is wrapped by `each_chunk` and `each_chunk_mapped`, and it allows you to
/// provide both the mapping function, and the chunk size.
///
/// Note: `mapped` basically just refers to the translating of `T` to `&dyn ToSql`
/// using the `to_sql` function. It's annoying that this is needed.
pub fn each_sized_chunk_mapped<'a, T, E, Mapper, DoChunk>(
    items: &'a [T],
    chunk_size: usize,
    to_sql: Mapper,
    mut do_chunk: DoChunk,
) -> Result<(), E>
where
    T: 'a,
    Mapper: Fn(&'a T) -> &'a dyn ToSql,
    DoChunk: FnMut(&[&dyn ToSql], usize) -> Result<(), E>,
{
    if items.is_empty() {
        return Ok(());
    }
    let mut vec = Vec::with_capacity(chunk_size.min(items.len()));
    let mut offset = 0;
    for chunk in items.chunks(chunk_size) {
        vec.clear();
        vec.extend(chunk.iter().map(|v| to_sql(v)));
        do_chunk(&vec, offset)?;
        offset += chunk.len();
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    fn check_chunk(items: &[&dyn ToSql], expect: &[impl ToSql], desc: &str) {
        assert_eq!(items.len(), expect.len());
        // Can't quite make the borrowing work out here w/o a loop, oh well.
        for (idx, (got, want)) in items.iter().zip(expect.iter()).enumerate() {
            assert_eq!(
                got.to_sql().unwrap(),
                want.to_sql().unwrap(),
                // ToSqlOutput::Owned(Value::Integer(*num)),
                "{}: Bad value at index {}",
                desc,
                idx
            );
        }
    }

    #[test]
    fn test_separate() {
        let mut iteration = 0;
        each_sized_chunk_mapped(
            &[1, 2, 3, 4, 5],
            3,
            |item| item as &dyn ToSql,
            |chunk, offset| {
                match offset {
                    0 => {
                        assert_eq!(iteration, 0);
                        check_chunk(chunk, &[1, 2, 3], "first chunk");
                    }
                    3 => {
                        assert_eq!(iteration, 1);
                        check_chunk(chunk, &[4, 5], "second chunk");
                    }
                    n => {
                        panic!("Unexpected offset {}", n);
                    }
                }
                iteration += 1;
                Ok::<(), ()>(())
            },
        )
        .unwrap();
    }

    #[test]
    fn test_leq_chunk_size() {
        for &check_size in &[5, 6] {
            let mut iteration = 0;
            each_sized_chunk_mapped(
                &[1, 2, 3, 4, 5],
                check_size,
                |item| item as &dyn ToSql,
                |chunk, offset| {
                    assert_eq!(iteration, 0);
                    iteration += 1;
                    assert_eq!(offset, 0);
                    check_chunk(chunk, &[1, 2, 3, 4, 5], "only iteration");
                    Ok::<(), ()>(())
                },
            )
            .unwrap();
        }
    }

    #[test]
    fn test_empty_chunk() {
        let items: &[i64] = &[];
        each_sized_chunk_mapped::<_, (), _, _>(
            items,
            100,
            |item| item as &dyn ToSql,
            |_, _| {
                panic!("Should never be called");
            },
        )
        .unwrap();
    }

    #[test]
    fn test_error() {
        let mut iteration = 0;
        let e = each_sized_chunk_mapped(
            &[1, 2, 3, 4, 5, 6, 7],
            3,
            |item| item as &dyn ToSql,
            |_, offset| {
                if offset == 0 {
                    assert_eq!(iteration, 0);
                    iteration += 1;
                    Ok(())
                } else if offset == 3 {
                    assert_eq!(iteration, 1);
                    iteration += 1;
                    Err("testing".to_string())
                } else {
                    // Make sure we stopped after the error.
                    panic!("Shouldn't get called with offset of {}", offset);
                }
            },
        )
        .expect_err("Should be an error");
        assert_eq!(e, "testing");
    }

}
