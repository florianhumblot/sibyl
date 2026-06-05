#[cfg(feature="blocking")]
mod blocking_tests {
    use sibyl::*;

    #[test]
    fn batch_insert_numeric() -> Result<()> {
        let session = sibyl::test_env::get_session()?;

        let stmt = session.prepare("
            INSERT INTO test_numeric_data (num, flt, dbl)
            VALUES (:num, :flt, :dbl)
        ")?;

        let nums: &[i32] = &[101, 102, 103];
        let flts: &[f32] = &[1.1, 2.2, 3.3];
        let dbls: &[f64] = &[1.11, 2.22, 3.33];

        let rows = stmt.execute_batch(3, (
            (":num", nums),
            (":flt", flts),
            (":dbl", dbls),
        ))?;
        assert_eq!(rows, 3);

        session.rollback()?;
        Ok(())
    }

    #[test]
    fn batch_insert_strings() -> Result<()> {
        let session = sibyl::test_env::get_session()?;

        let stmt = session.prepare("
            INSERT INTO test_character_data (text)
            VALUES (:text)
        ")?;

        let texts: &[&str] = &["first", "second_long_string", "third"];

        let rows = stmt.execute_batch(3, (":text", texts))?;
        assert_eq!(rows, 3);

        session.rollback()?;
        Ok(())
    }

    #[test]
    fn batch_insert_returning() -> Result<()> {
        let session = sibyl::test_env::get_session()?;

        let stmt = session.prepare("
            INSERT INTO test_numeric_data (num)
            VALUES (:num)
            RETURNING id INTO :id
        ")?;

        let nums: &[i32] = &[201, 202, 203];
        let mut ids = [0, 0, 0];

        let rows = stmt.execute_batch(3, (
            (":num", nums),
            (":id", &mut ids[..]),
        ))?;
        assert_eq!(rows, 3);

        for &id in ids.iter() {
            assert!(id > 0);
        }

        // Test updating returning
        let update_stmt = session.prepare("
            UPDATE test_numeric_data
            SET num = :num_new
            WHERE id = :id
            RETURNING num INTO :num_out
        ")?;

        let nums_new: &[i32] = &[301, 302, 303];
        let mut nums_out = [0, 0, 0];

        let rows_updated = update_stmt.execute_batch(3, (
            (":num_new", nums_new),
            (":id", &ids[..]),
            (":num_out", &mut nums_out[..]),
        ))?;
        assert_eq!(rows_updated, 3);
        assert_eq!(nums_out, [301, 302, 303]);

        session.rollback()?;
        Ok(())
    }

    #[test]
    fn batch_insert_nulls() -> Result<()> {
        let session = sibyl::test_env::get_session()?;

        let stmt = session.prepare("
            INSERT INTO test_numeric_data (num)
            VALUES (:num)
        ")?;

        let nums: &[Option<i32>] = &[Some(401), None, Some(403)];

        let rows = stmt.execute_batch(3, (":num", nums))?;
        assert_eq!(rows, 3);

        session.rollback()?;
        Ok(())
    }

    #[test]
    fn batch_mismatched_lengths() -> Result<()> {
        let session = sibyl::test_env::get_session()?;

        let stmt = session.prepare("
            INSERT INTO test_numeric_data (num, flt)
            VALUES (:num, :flt)
        ")?;

        let nums: &[i32] = &[101, 102, 103];
        let flts: &[f32] = &[1.1, 2.2]; // Length 2, batch_size 3

        let res = stmt.execute_batch(3, (
            (":num", nums),
            (":flt", flts),
        ));
        assert!(res.is_err());

        Ok(())
    }

    #[test]
    fn batch_constraint_violation() -> Result<()> {
        let session = sibyl::test_env::get_session()?;

        let stmt = session.prepare("
            INSERT INTO hr.locations (location_id, city)
            VALUES (:id, :city)
        ")?;

        let ids: &[i32] = &[9999, 9999]; // Duplicate ID -> Unique constraint violation
        let cities: &[&str] = &["CityA", "CityB"];

        let res = stmt.execute_batch(2, (
            (":id", ids),
            (":city", cities),
        ));
        assert!(res.is_err());

        session.rollback()?;
        Ok(())
    }

    #[test]
    fn batch_size_one() -> Result<()> {
        let session = sibyl::test_env::get_session()?;

        let stmt = session.prepare("
            INSERT INTO test_numeric_data (num)
            VALUES (:num)
        ")?;

        let nums: &[i32] = &[501];

        let rows = stmt.execute_batch(1, (":num", nums))?;
        assert_eq!(rows, 1);

        session.rollback()?;
        Ok(())
    }

    #[test]
    fn batch_size_zero() -> Result<()> {
        let session = sibyl::test_env::get_session()?;

        let stmt = session.prepare("
            INSERT INTO test_numeric_data (num)
            VALUES (:num)
        ")?;

        let nums: &[i32] = &[];

        let res = stmt.execute_batch(0, (":num", nums));
        assert!(res.is_err());

        Ok(())
    }

    #[test]
    fn batch_reuse_statement() -> Result<()> {
        let session = sibyl::test_env::get_session()?;

        let stmt = session.prepare("
            INSERT INTO test_numeric_data (num)
            VALUES (:num)
        ")?;

        let batch1: &[i32] = &[601, 602];
        let rows = stmt.execute_batch(2, (":num", batch1))?;
        assert_eq!(rows, 2);

        let batch2: &[i32] = &[603, 604, 605];
        let rows = stmt.execute_batch(3, (":num", batch2))?;
        assert_eq!(rows, 3);

        session.rollback()?;
        Ok(())
    }
}

#[cfg(feature="nonblocking")]
mod nonblocking_tests {
    use sibyl::*;

    #[test]
    fn batch_insert_numeric() -> Result<()> {
        block_on(async {
            let session = sibyl::test_env::get_session().await?;

            let stmt = session.prepare("
                INSERT INTO test_numeric_data (num, flt, dbl)
                VALUES (:num, :flt, :dbl)
            ").await?;

            let nums: &[i32] = &[101, 102, 103];
            let flts: &[f32] = &[1.1, 2.2, 3.3];
            let dbls: &[f64] = &[1.11, 2.22, 3.33];

            let rows = stmt.execute_batch(3, (
                (":num", nums),
                (":flt", flts),
                (":dbl", dbls),
            )).await?;
            assert_eq!(rows, 3);

            session.rollback().await?;
            Ok(())
        })
    }

    #[test]
    fn batch_insert_strings() -> Result<()> {
        block_on(async {
            let session = sibyl::test_env::get_session().await?;

            let stmt = session.prepare("
                INSERT INTO test_character_data (text)
                VALUES (:text)
            ").await?;

            let texts: &[&str] = &["first", "second_long_string", "third"];

            let rows = stmt.execute_batch(3, (":text", texts)).await?;
            assert_eq!(rows, 3);

            session.rollback().await?;
            Ok(())
        })
    }

    #[test]
    fn batch_insert_returning() -> Result<()> {
        block_on(async {
            let session = sibyl::test_env::get_session().await?;

            let stmt = session.prepare("
                INSERT INTO test_numeric_data (num)
                VALUES (:num)
                RETURNING id INTO :id
            ").await?;

            let nums: &[i32] = &[201, 202, 203];
            let mut ids = [0, 0, 0];

            let rows = stmt.execute_batch(3, (
                (":num", nums),
                (":id", &mut ids[..]),
            )).await?;
            assert_eq!(rows, 3);

            for &id in ids.iter() {
                assert!(id > 0);
            }

            // Test updating returning
            let update_stmt = session.prepare("
                UPDATE test_numeric_data
                SET num = :num_new
                WHERE id = :id
                RETURNING num INTO :num_out
            ").await?;

            let nums_new: &[i32] = &[301, 302, 303];
            let mut nums_out = [0, 0, 0];

            let rows_updated = update_stmt.execute_batch(3, (
                (":num_new", nums_new),
                (":id", &ids[..]),
                (":num_out", &mut nums_out[..]),
            )).await?;
            assert_eq!(rows_updated, 3);
            assert_eq!(nums_out, [301, 302, 303]);

            session.rollback().await?;
            Ok(())
        })
    }

    #[test]
    fn batch_insert_nulls() -> Result<()> {
        block_on(async {
            let session = sibyl::test_env::get_session().await?;

            let stmt = session.prepare("
                INSERT INTO test_numeric_data (num)
                VALUES (:num)
            ").await?;

            let nums: &[Option<i32>] = &[Some(401), None, Some(403)];

            let rows = stmt.execute_batch(3, (":num", nums)).await?;
            assert_eq!(rows, 3);

            session.rollback().await?;
            Ok(())
        })
    }

    #[test]
    fn batch_mismatched_lengths() -> Result<()> {
        block_on(async {
            let session = sibyl::test_env::get_session().await?;

            let stmt = session.prepare("
                INSERT INTO test_numeric_data (num, flt)
                VALUES (:num, :flt)
            ").await?;

            let nums: &[i32] = &[101, 102, 103];
            let flts: &[f32] = &[1.1, 2.2]; // Length 2, batch_size 3

            let res = stmt.execute_batch(3, (
                (":num", nums),
                (":flt", flts),
            )).await;
            assert!(res.is_err());

            Ok(())
        })
    }

    #[test]
    fn batch_constraint_violation() -> Result<()> {
        block_on(async {
            let session = sibyl::test_env::get_session().await?;

            let stmt = session.prepare("
                INSERT INTO hr.locations (location_id, city)
                VALUES (:id, :city)
            ").await?;

            let ids: &[i32] = &[9999, 9999]; // Duplicate ID -> Unique constraint violation
            let cities: &[&str] = &["CityA", "CityB"];

            let res = stmt.execute_batch(2, (
                (":id", ids),
                (":city", cities),
            )).await;
            assert!(res.is_err());

            session.rollback().await?;
            Ok(())
        })
    }

    #[test]
    fn batch_size_one() -> Result<()> {
        block_on(async {
            let session = sibyl::test_env::get_session().await?;

            let stmt = session.prepare("
                INSERT INTO test_numeric_data (num)
                VALUES (:num)
            ").await?;

            let nums: &[i32] = &[501];

            let rows = stmt.execute_batch(1, (":num", nums)).await?;
            assert_eq!(rows, 1);

            session.rollback().await?;
            Ok(())
        })
    }

    #[test]
    fn batch_size_zero() -> Result<()> {
        block_on(async {
            let session = sibyl::test_env::get_session().await?;

            let stmt = session.prepare("
                INSERT INTO test_numeric_data (num)
                VALUES (:num)
            ").await?;

            let nums: &[i32] = &[];

            let res = stmt.execute_batch(0, (":num", nums)).await;
            assert!(res.is_err());

            Ok(())
        })
    }

    #[test]
    fn batch_reuse_statement() -> Result<()> {
        block_on(async {
            let session = sibyl::test_env::get_session().await?;

            let stmt = session.prepare("
                INSERT INTO test_numeric_data (num)
                VALUES (:num)
            ").await?;

            let batch1: &[i32] = &[601, 602];
            let rows = stmt.execute_batch(2, (":num", batch1)).await?;
            assert_eq!(rows, 2);

            let batch2: &[i32] = &[603, 604, 605];
            let rows = stmt.execute_batch(3, (":num", batch2)).await?;
            assert_eq!(rows, 3);

            session.rollback().await?;
            Ok(())
        })
    }
}
