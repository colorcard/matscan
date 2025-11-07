use std::collections::HashSet;

use tracing::info;

use crate::{
    database::{Database, collect_servers::CollectServersFilter},
    scanner::targets::ScanRange,
    strategies::slash16_a,
};

pub async fn get_ranges(database: &Database) -> eyre::Result<Vec<ScanRange>> {
    let known_servers = database
        .collect_all_servers(CollectServersFilter::Active30d)
        .await?;

    let known_ips = known_servers
        .iter()
        .map(|target| target.ip())
        .collect::<HashSet<_>>();
    info!("Total unique ips: {}", known_ips.len());

    let mut target_ranges = Vec::new();

    for &address in known_ips {
        target_ranges.push(ScanRange {
            ip_start: address,
            ip_end: address,
            port_start: 1024,
            port_end: 65535,
        });
    }

    // also scan slash16_a at the same time to avoid overwhelming our targets
    target_ranges.extend(slash16_a::get_ranges(database).await?);

    Ok(target_ranges)
}
