//! Scan every 1024-65535 on IPs that have at least 8 servers that've been
//! active in the past 30 days.

use std::net::Ipv4Addr;

use rustc_hash::FxHashMap;
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

    let mut known_ips_and_counts = FxHashMap::<Ipv4Addr, usize>::default();
    for target in known_servers.iter() {
        *known_ips_and_counts.entry(*target.ip()).or_insert(0) += 1;
    }
    info!("Total unique ips: {}", known_ips_and_counts.len());

    let mut target_ranges = Vec::new();

    for (address, count) in known_ips_and_counts {
        if count < 8 {
            continue;
        }
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
