use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

pub async fn connect_with_retries<T, F, Fut>(label: &str, mut connect: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<T, sqlx::Error>>,
{
    let max_retries = 5;
    let retry_delay = Duration::from_secs(2);

    for attempt in 1..=max_retries {
        match connect().await {
            Ok(connection) => return connection,
            Err(error) if attempt < max_retries => {
                eprintln!(
                    "{} connection attempt {}/{} failed: {}. Retrying in {:?}...",
                    label, attempt, max_retries, error, retry_delay
                );
                tokio::time::sleep(retry_delay).await;
            }
            Err(error) => {
                panic!(
                    "Failed to connect to {} after {} attempts. Error: {}",
                    label, max_retries, error
                );
            }
        }
    }

    unreachable!()
}

pub fn unique_suffix() -> String {
    Uuid::new_v4().to_string().replace('-', "")[..8].to_string()
}

pub fn migration_names(paths: Vec<PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown")
                .to_string()
        })
        .collect()
}
