#![allow(unused_imports)]
use async_std::fs::{File, OpenOptions};
use async_std::io::prelude::*;
use async_std::io::{BufReader, BufWriter, Write};
use async_std::sync::{Arc, Mutex};
use futures::future::{join3, join_all};
use futures::Future;
use isahc::prelude::*;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::to_string_pretty;
use serde_json::{from_reader, to_string};
use std::collections::vec_deque::VecDeque;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use structopt::StructOpt;

use futures::executor::block_on;
use peertube_lib::instance_storage::InstanceDb;
use peertube_lib::peertube_api::fetch_instance_list_from_joinpeertube;
use peertube_lib::peertube_api::Video;
use stderrlog;

type BoxedFuture = Box<dyn Future<Output = ()>>;

const OUTPUT_DIR: &str = "crawled/";

const LIMIT: u64 = 0;

#[derive(Debug, Hash, Clone, Serialize, Deserialize)]
struct APIInstance {
    name: String,
    followers: Vec<String>,
    following: Vec<String>,
}

impl APIInstance {
    pub fn new(name: String) -> APIInstance {
        APIInstance {
            name: name,
            followers: vec![],
            following: vec![],
        }
    }
}

impl PartialEq for APIInstance {
    fn eq(&self, other: &APIInstance) -> bool {
        self.name == other.name
    }
}

impl Eq for APIInstance {}

async fn queue_for_crawling(
    item: String,
    nodes: Arc<Mutex<HashSet<String>>>,
    result: Arc<Mutex<HashSet<APIInstance>>>,
    count: Arc<Mutex<u64>>,
    db: Arc<Mutex<InstanceDb>>,
    http_client: Arc<HttpClient>,
) -> BoxedFuture {
    db.lock().await.insert_instance(item.clone());
    if !nodes.lock().await.contains(&item) {
        nodes.lock().await.insert(item.clone());
        let nodes_clone = nodes.clone();
        let result_clone = result.clone();
        if *(count.lock().await) < LIMIT || LIMIT == 0 {
            *(count.lock().await) += 1;
            Box::new(fetch(
                item,
                nodes_clone,
                result_clone,
                count,
                db,
                http_client,
            ))
        } else {
            Box::new(async { () })
        }
    } else {
        Box::new(async { () })
    }
}

async fn write_to_file(filename: String, data: Vec<Video>) {
    if let Ok(file) = File::create(filename.clone()).await {
        let lines: String = data.into_iter().fold("".to_string(), |mut acc, o| {
            acc += &to_string(&o).unwrap();
            acc += "\n";
            acc
        });
        let mut writer = BufWriter::new(file);
        if let Err(e) = writer.write(&(lines.as_bytes())).await {
            error!("Error while writing videos to {} : {}", filename, e);
        }
    }
}

async fn crawl_from_instances(
    instances: Vec<String>,
    nodes: Arc<Mutex<HashSet<String>>>,
    result: Arc<Mutex<HashSet<APIInstance>>>,
    count: Arc<Mutex<u64>>,
    db: Arc<Mutex<InstanceDb>>,
    http_client: Arc<HttpClient>,
) {
    let mut futures = vec![];
    for instance in instances {
        let f = fetch(
            instance,
            nodes.clone(),
            result.clone(),
            count.clone(),
            db.clone(),
            http_client.clone(),
        );
        futures.push(f);
    }
    join_all(futures).await;
}

async fn fetch_video(name: String, http_client: Arc<HttpClient>) {
    let instance_url = "https://".to_owned() + name.clone().as_str();
    let query_videos = instance_url.clone() + "/api/v1/videos?count=50000&start=0&nsfw=true";
    let filename = OUTPUT_DIR.to_owned() + &name + ".json";
    let request = Request::get(&query_videos).body(()).unwrap();
    if let Ok(mut req) = http_client.send_async(request).await {
        if let Ok(json) = req.json::<serde_json::Value>() {
            let mut result = vec![];
            if let Some(data) = json["data"].as_array() {
                for value in data.into_iter() {
                    match serde_json::from_value(value.clone()) {
                        Ok(video) => result.push(video),
                        Err(e) => trace!("Failed to parse peertube response : {}", e),
                    }
                }
            }
            /*let database = Database::default();
            process_videos(database, result);*/
            write_to_file(filename, result).await;
        }
    }
}

async fn fetch_follow(
    api_endpoint: &'static str,
    entry_name: &'static str,
    instance: Arc<Mutex<APIInstance>>,
    name: String,
    nodes: Arc<Mutex<HashSet<String>>>,
    result: Arc<Mutex<HashSet<APIInstance>>>,
    count: Arc<Mutex<u64>>,
    db: Arc<Mutex<InstanceDb>>,
    http_client: Arc<HttpClient>,
) -> BoxedFuture {
    let query = "https://".to_owned() + name.clone().as_str() + "/api/v1" + api_endpoint;
    let request = Request::get(&query).body(()).unwrap();
    let mut tasks = Vec::new();
    if let Ok(mut req) = http_client.send_async(request).await {
        if let Ok(json) = req.json::<serde_json::Value>() {
            if let Some(data) = json["data"].as_array() {
                for entry in data {
                    if let Some(hostname) = entry[entry_name]["host"].as_str() {
                        if hostname != name {
                            tasks.push(queue_for_crawling(
                                hostname.to_string(),
                                nodes.clone(),
                                result.clone(),
                                count.clone(),
                                db.clone(),
                                http_client.clone(),
                            ));
                            instance.lock().await.followers.push(hostname.to_owned());
                        }
                    }
                }
            }
        }
    }
    Box::new(async {
        join_all(tasks).await;
        ()
    })
}

async fn fetch(
    name: String,
    nodes: Arc<Mutex<HashSet<String>>>,
    result: Arc<Mutex<HashSet<APIInstance>>>,
    count: Arc<Mutex<u64>>,
    db: Arc<Mutex<InstanceDb>>,
    http_client: Arc<HttpClient>,
) {
    trace!("Processing instance : {}", name);
    let instance = Arc::new(Mutex::new(APIInstance::new(name.clone())));
    // Request ressources from host
    let t0 = fetch_follow(
        "/server/following",
        "following",
        instance.clone(),
        name.clone(),
        nodes.clone(),
        result.clone(),
        count.clone(),
        db.clone(),
        http_client.clone(),
    );

    let t1 = fetch_follow(
        "/server/followers",
        "followers",
        instance.clone(),
        name.clone(),
        nodes.clone(),
        result.clone(),
        count.clone(),
        db.clone(),
        http_client.clone(),
    );

    let t2 = fetch_video(name, http_client);

    join3(t0, t1, t2).await;
    /*
            let f = t0
                .join(t1)
                .and_then(move |(val, _): (Arc<Mutex<APIInstance>>, _)| {
                    result.lock().unwrap().insert(val.lock().unwrap().clone());
                    ok(())
                });
            tokio::spawn(f);
            ok(())
        })
    */
}

async fn crawl() {
    // TODO : use SegQueue
    let nodes = Arc::new(Mutex::new(HashSet::new()));
    let result: Arc<Mutex<HashSet<APIInstance>>> = Arc::new(Mutex::new(HashSet::new()));
    let count = Arc::new(Mutex::new(0));

    let instance_db = Arc::new(Mutex::new(InstanceDb::new()));
    let mut instances = instance_db.lock().await.get_all_instances();

    for instance in &instances {
        nodes.lock().await.insert(instance.clone());
    }

    if nodes.lock().await.len() == 0 {
        info!("No instance found in the database, seeding from https://instances.joinpeertube.org");
        match fetch_instance_list_from_joinpeertube() {
            Ok(res) => {
                info!("Fetched {} instances", res.len());
                for s in res {
                    instances.push(s);
                }
            }
            Err(e) => warn!(
                "Failed to retrieve instances from https://instances.joinpeertube.org : {}",
                e
            ),
        }
    }
    info!(
        "Starting crawling process from {} instances",
        instances.len()
    );
    let start = Instant::now();

    let client = HttpClient::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(5))
        .connection_cache_size(4096 * 100_000_000) /* 100 MB cache */
        .build()
        .unwrap();

    crawl_from_instances(
        instances,
        nodes.clone(),
        result.clone(),
        count,
        instance_db.clone(),
        Arc::new(client),
    )
    .await;
    let duration = start.elapsed();
    info!("Found {} instances", nodes.lock().await.len());
    info!(
        "Added {} instances in {} seconds",
        instance_db.lock().await.get_instance_added(),
        duration.as_secs()
    );
}

fn elastic_is_online() -> bool {
    /*
    let client = SyncClientBuilder::new()
        .build()
        .expect("Failed to initialize elastic client");
    if let Ok(resp) = client.ping().send() {
        info!(
            "Elastic search is online : connected to {}@{}",
            resp.name(),
            resp.cluster_name(),
        );
        true
    } else {
        false
    }*/
    true
}

#[derive(StructOpt, Debug)]
#[structopt()]
struct Opt {
    /// Silence all output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,
    /// Verbose mode (-v, -vv, -vvv, etc)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,
    /// Timestamp (sec, ms, ns, none)
    #[structopt(short = "t", long = "timestamp")]
    ts: Option<stderrlog::Timestamp>,
}

fn main() -> Result<(), ()> {
    let opt = Opt::from_args();

    stderrlog::new()
        .module(module_path!())
        .quiet(opt.quiet)
        .verbosity(opt.verbose)
        .timestamp(opt.ts.unwrap_or(stderrlog::Timestamp::Off))
        .init()
        .unwrap();

    if elastic_is_online() {
        block_on(crawl());
        Ok(())
    } else {
        error!("Failed to connect to elastic instance");
        Err(())
    }
}
