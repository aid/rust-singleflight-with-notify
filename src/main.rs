use std::collections::{HashMap, HashSet};
use std::env;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

#[derive(Default, Debug, Clone)]
struct DnsCache {
    // Map of resolved hostnames.
    data: Arc<RwLock<HashMap<String, ResolvedDns>>>,
}

#[derive(Default, Debug, Clone)]
struct DnsResolver {
    cache: DnsCache,
    // Map of in-progress resolution requests.
    in_progress: Arc<RwLock<HashMap<String, Arc<Notify>>>>,
}

#[allow(dead_code)]
#[derive(Default, Debug, Clone)]
struct ResolvedDns {
    hostname: String,
    ips: HashSet<IpAddr>,
    initial_query: Option<std::time::Instant>,
    dns_refresh_rate: std::time::Duration,
}

impl DnsCache {
    fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn get(&self, hostname: &str) -> Option<ResolvedDns> {
        self.data
            .read()
            .unwrap()
            .get(hostname)
            .filter(|rdns| {
                rdns.initial_query.is_some()
                    && rdns.initial_query.unwrap().elapsed() < rdns.dns_refresh_rate
            })
            .cloned()
    }

    fn set(&self, hostname: &String, resolved_dns: &Option<ResolvedDns>) {
        if let Some(dns) = resolved_dns {
            let mut data_map = self.data.write().unwrap();
            data_map.insert(hostname.clone(), dns.clone());
        }
    }
}

impl DnsResolver {
    fn new() -> Self {
        Self {
            cache: DnsCache::new(),
            in_progress: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn resolve_host(&self, hostname: String) -> Option<ResolvedDns> {
        // Serve from the local cache if we can...
        if let Some(resolved_dns) = self.cache.get(&hostname) {
            Some(resolved_dns)
        } else {
            // No cache entry so we need to perform the DNS lookup and
            // update the cache...
            if let Some(notify) = self.get_notify(&hostname) {
                // If we're already looking up this DNS entry, let's just
                // wait on that completing and then return the cache entry...
                notify.notified().await;
                self.cache.get(&hostname)
            } else {
                // No current DNS lookup for this domain, so let's get that done...

                // Record that we're currently looking up this domain
                let notify = self.create_notify(&hostname);

                // Perform the actual DNS lookup
                let resolved_dns = self.resolve_on_demand_dns(&hostname).await;

                // Cache the response
                self.cache.set(&hostname, &resolved_dns);

                // As the resolution is complete, we can remove the in-progress
                // notify object.
                self.remove_notify(&hostname);

                // Notify all waiters after the DNS resolving task completed.
                notify.notify_waiters();

                // We have the result; so can return directly and don't need
                // to hit the cache for this...
                resolved_dns
            }
        }
    }

    fn get_notify(&self, hostname: &str) -> Option<Arc<Notify>> {
        let in_progress = self.in_progress.read().unwrap();
        in_progress.get(hostname).map(|notify| notify.clone())
    }

    fn create_notify(&self, hostname: &String) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        let mut in_progress = self.in_progress.write().unwrap();
        in_progress.insert(hostname.clone(), notify.clone());
        notify
    }

    fn remove_notify(&self, hostname: &str) {
        self.in_progress.write().unwrap().remove(hostname);
    }

    async fn resolve_on_demand_dns(&self, hostname: &String) -> Option<ResolvedDns> {
        // Simulated DNS resolution delay
        thread::sleep(Duration::from_secs(2));
        // Here you would perform the actual DNS resolution
        Some(ResolvedDns {
            hostname: hostname.clone(),
            ips: HashSet::new(), // Placeholder for resolved IPs
            initial_query: Some(std::time::Instant::now()),
            dns_refresh_rate: Duration::from_secs(60), // Example refresh rate
        })
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    // Find the concurrency parameter
    let concurrency_value = args
        .iter()
        .position(|arg| arg == "--concurrency")
        .and_then(|index| args.get(index + 1))
        .expect("Concurrency parameter not provided");

    let concurrency: i32 = concurrency_value
        .parse()
        .expect("Invalid concurrency parameter value");

    let resolver = DnsResolver::new();

    let start = Instant::now();
    // Spawn multiple tasks to simulate concurrent DNS resolution requests.
    let tasks = (0..concurrency)
        .map(|_i| {
            let resolver = resolver.clone();
            tokio::spawn(async move {
                let hostname = format!("example.com");
                let result = resolver.resolve_host(hostname).await;
                assert!(result.is_some());
                assert_eq!(result.unwrap().hostname, String::from("example.com"));
            })
        })
        .collect::<Vec<_>>();

    // Wait for all tasks to complete.
    for task in tasks {
        let _ = task.await;
    }
    let end = Instant::now();
    let duration = end - start;
    println!("Time taken: {:?}", duration);
}
