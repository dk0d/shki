macro_rules! with_tx {
    ($pool:expr, |$tx:ident|  $body:block) => {{
        let mut $tx = $pool
            .begin()
            .await
            .map_err(|e| ShkiError::migration(format!("Failed to begin transaction: {e}")))?;

        let result = async { $body }.await;

        match result {
            Ok(value) => {
                $tx.commit().await.map_err(|e| {
                    ShkiError::migration(format!("Failed to commit transaction: {e}"))
                })?;
                Ok(value)
            }
            Err(err) => Err(err),
        }
    }};
}

pub(crate) use with_tx;
