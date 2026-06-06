use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono::Utc;
use futures_util::{stream::FuturesUnordered, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use serenity::all::{ChannelId, Context, CreateEmbed, CreateEmbedFooter, CreateMessage};
use tokio::sync::Semaphore;

use crate::db::Db;

#[derive(Debug, Clone)]
pub struct OsRelease {
    pub id: String,
    pub product: String,
    pub family: String,
    pub cycle: String,
    pub latest: String,
    pub release_date: String,
    pub url: String,
    pub summary: String,
}

#[derive(Debug, Clone, Copy)]
struct Product {
    slug: &'static str,
    label: &'static str,
    family: &'static str,
}

const PRODUCTS: &[Product] = &[
    Product {
        slug: "almalinux",
        label: "AlmaLinux",
        family: "Linux",
    },
    Product {
        slug: "alpine-linux",
        label: "Alpine Linux",
        family: "Linux",
    },
    Product {
        slug: "amazon-linux",
        label: "Amazon Linux",
        family: "Linux",
    },
    Product {
        slug: "antix",
        label: "antiX",
        family: "Linux",
    },
    Product {
        slug: "centos",
        label: "CentOS",
        family: "Linux",
    },
    Product {
        slug: "centos-stream",
        label: "CentOS Stream",
        family: "Linux",
    },
    Product {
        slug: "clear-linux",
        label: "Clear Linux",
        family: "Linux",
    },
    Product {
        slug: "cos",
        label: "Container-Optimized OS",
        family: "Linux",
    },
    Product {
        slug: "debian",
        label: "Debian",
        family: "Linux",
    },
    Product {
        slug: "devuan",
        label: "Devuan",
        family: "Linux",
    },
    Product {
        slug: "eurolinux",
        label: "EuroLinux",
        family: "Linux",
    },
    Product {
        slug: "fedora",
        label: "Fedora",
        family: "Linux",
    },
    Product {
        slug: "linux",
        label: "Linux Kernel",
        family: "Linux",
    },
    Product {
        slug: "linuxmint",
        label: "Linux Mint",
        family: "Linux",
    },
    Product {
        slug: "mageia",
        label: "Mageia",
        family: "Linux",
    },
    Product {
        slug: "mxlinux",
        label: "MX Linux",
        family: "Linux",
    },
    Product {
        slug: "nixos",
        label: "NixOS",
        family: "Linux",
    },
    Product {
        slug: "opensuse",
        label: "openSUSE",
        family: "Linux",
    },
    Product {
        slug: "openwrt",
        label: "OpenWrt",
        family: "Linux",
    },
    Product {
        slug: "oracle-linux",
        label: "Oracle Linux",
        family: "Linux",
    },
    Product {
        slug: "photon",
        label: "VMware Photon OS",
        family: "Linux",
    },
    Product {
        slug: "pop-os",
        label: "Pop!_OS",
        family: "Linux",
    },
    Product {
        slug: "postmarketos",
        label: "postmarketOS",
        family: "Linux",
    },
    Product {
        slug: "proxmox-ve",
        label: "Proxmox VE",
        family: "Linux",
    },
    Product {
        slug: "raspberry-pi",
        label: "Raspberry Pi OS",
        family: "Linux",
    },
    Product {
        slug: "rhel",
        label: "RHEL",
        family: "Linux",
    },
    Product {
        slug: "rocky-linux",
        label: "Rocky Linux",
        family: "Linux",
    },
    Product {
        slug: "slackware",
        label: "Slackware",
        family: "Linux",
    },
    Product {
        slug: "sles",
        label: "SLES",
        family: "Linux",
    },
    Product {
        slug: "steamos",
        label: "SteamOS",
        family: "Linux",
    },
    Product {
        slug: "suse-linux-micro",
        label: "SUSE Linux Micro",
        family: "Linux",
    },
    Product {
        slug: "tails",
        label: "Tails",
        family: "Linux",
    },
    Product {
        slug: "ubuntu",
        label: "Ubuntu",
        family: "Linux",
    },
    Product {
        slug: "yocto",
        label: "Yocto",
        family: "Linux",
    },
    Product {
        slug: "zentyal",
        label: "Zentyal",
        family: "Linux",
    },
    Product {
        slug: "windows",
        label: "Windows",
        family: "Windows",
    },
    Product {
        slug: "macos",
        label: "macOS",
        family: "macOS",
    },
    Product {
        slug: "freebsd",
        label: "FreeBSD",
        family: "BSD",
    },
    Product {
        slug: "openbsd",
        label: "OpenBSD",
        family: "BSD",
    },
    Product {
        slug: "netbsd",
        label: "NetBSD",
        family: "BSD",
    },
];

#[derive(Debug, Deserialize)]
struct EolCycle {
    cycle: String,
    #[serde(default)]
    latest: Option<String>,
    #[serde(default, rename = "latestReleaseDate")]
    latest_release_date: Option<String>,
    #[serde(default, rename = "releaseDate")]
    release_date: Option<String>,
}

pub async fn fetch_releases(http: &Client) -> Result<Vec<OsRelease>> {
    let today = Utc::now().date_naive().to_string();
    let mut releases = Vec::new();
    let gate = Arc::new(Semaphore::new(12));
    let mut tasks = FuturesUnordered::new();

    for product in PRODUCTS {
        let http = http.clone();
        let gate = gate.clone();
        let product = *product;
        tasks.push(async move {
            let _permit = gate.acquire_owned().await?;
            fetch_product(&http, product).await
        });
    }

    while let Some(result) = tasks.next().await {
        let (product, cycles) = result?;
        for cycle in cycles {
            let latest = cycle.latest.unwrap_or_else(|| cycle.cycle.clone());
            let release_date = cycle
                .latest_release_date
                .or(cycle.release_date)
                .unwrap_or_else(|| "unknown".to_string());
            if release_date != today {
                continue;
            }

            let summary = format!(
                "{} {} released latest build/version {} on {}.",
                product.label, cycle.cycle, latest, release_date
            );

            releases.push(OsRelease {
                id: format!("{}:{}:{}", product.slug, cycle.cycle, latest),
                product: product.label.to_string(),
                family: product.family.to_string(),
                cycle: cycle.cycle,
                latest,
                release_date,
                url: format!("https://endoflife.date/{}", product.slug),
                summary,
            });
        }
    }

    releases.sort_by(|a, b| b.release_date.cmp(&a.release_date));
    Ok(releases)
}

pub async fn store_new(db: &Db, items: &[OsRelease]) -> Result<Vec<OsRelease>> {
    let mut fresh = Vec::new();

    for item in items {
        let inserted = db
            .insert_seen(
                "os_release",
                &item.id,
                &item.summary,
                &item.family,
                &item.url,
                &item.release_date,
            )
            .await?;

        if inserted {
            fresh.push(item.clone());
        }
    }
    Ok(fresh)
}

pub async fn send_releases(ctx: &Context, channel_id: u64, items: &[OsRelease]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }

    for chunk in items.chunks(12) {
        let linux_count = chunk.iter().filter(|item| item.family == "Linux").count();
        let mut embed = CreateEmbed::new()
            .title("OS Releases Today")
            .description(format!(
                "UTC today only. Sources: popular Linux distributions plus Windows, macOS, and BSD. Linux items in this embed: {linux_count}."
            ))
            .color(0x2d7dd2)
            .footer(CreateEmbedFooter::new(format!(
                "{} release item(s)",
                chunk.len()
            )));

        for item in chunk {
            embed = embed.field(
                format!("{} {}", item.product, item.cycle),
                format!(
                    "[{}]({}) | {} | release date: {}\nSummary: {}",
                    item.latest, item.url, item.family, item.release_date, item.summary
                ),
                false,
            );
        }

        ChannelId::new(channel_id)
            .send_message(&ctx.http, CreateMessage::new().embed(embed))
            .await?;
    }

    Ok(())
}

async fn fetch_product(http: &Client, product: Product) -> Result<(Product, Vec<EolCycle>)> {
    let url = format!("https://endoflife.date/api/{}.json", product.slug);
    let cycles = http
        .get(&url)
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("{} API returned an error", product.label))?
        .json::<Vec<EolCycle>>()
        .await?;

    Ok((product, cycles))
}
