#![allow(unused_imports)]
use async_std::fs::{File, OpenOptions};
use async_std::io::prelude::*;
use async_std::io::{BufReader, BufWriter, Write};
use async_std::sync::{Arc, Mutex};
use futures::future::{join3, join_all};
use futures::{Future, FutureExt};
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
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use peertube_lib::elastic::create_mappings;
use peertube_lib::instance_storage::InstanceDb;
use peertube_lib::peertube_api::fetch_instance_list_from_joinpeertube;
use peertube_lib::peertube_api::Video;
use std::cmp::min;
use std::convert::TryInto;
use std::f32::MAX;
use std::hash::{Hash, Hasher};
use std::io::Stdout;
use std::path::Path;
use std::pin::Pin;
use stderrlog;
use stderrlog::ColorChoice;

const OUTPUT_DIR: &str = "crawled/";

/** Maximum number of videos to fetch from an instance */
const MAX_VIDEOS: u64 = 100;

const LIMIT: u64 = 0;

#[derive(Clone)]
struct CrawlCtx {
    pub nodes: Arc<Mutex<HashSet<String>>>,
    pub result: Arc<Mutex<HashSet<APIInstance>>>,
    pub count: Arc<Mutex<u64>>,
    pub db: Arc<Mutex<InstanceDb>>,
    pub http_client: Arc<HttpClient>,
    pub instance_bar: ProgressBar,
    pub video_bar: ProgressBar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct APIInstance {
    name: String,
    followers: Vec<String>,
    following: Vec<String>,
}

impl APIInstance {
    pub fn new(name: String) -> APIInstance {
        APIInstance {
            name,
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

impl Hash for APIInstance {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}
impl Eq for APIInstance {}

async fn queue_for_crawling(item: String, ctx: CrawlCtx) -> Pin<Box<dyn Future<Output = ()>>> {
    let mut res: Pin<Box<dyn Future<Output = ()>>> = Box::pin(async {});
    ctx.db.lock().await.insert_instance(item.clone());
    if !ctx.nodes.lock().await.contains(&item) {
        ctx.nodes.lock().await.insert(item.clone());
        let mut count_val = ctx.count.lock().await;
        if *count_val < LIMIT || LIMIT == 0 {
            *count_val += 1;
            ctx.instance_bar.inc_length(1);
            trace!("[{}] Scheduled", item);
            res = Box::pin(fetch(item, ctx.clone()));
        } else {
            //trace!("[{}] Skipped : reached maximum depth", item);
        }
    } else {
        //trace!("[{}] Skipped: already in queue", item);
    }
    res
}

async fn write_to_file(filename: String, data: Vec<Video>) {
    match File::create(filename.clone()).await {
        Ok(file) => {
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
        Err(e) => error!("{}", e),
    }
}

async fn crawl_from_instances(
    instances: Vec<String>,
    nodes: Arc<Mutex<HashSet<String>>>,
    result: Arc<Mutex<HashSet<APIInstance>>>,
    count: Arc<Mutex<u64>>,
    db: Arc<Mutex<InstanceDb>>,
    http_client: Arc<HttpClient>,
    instance_bar: ProgressBar,
    video_bar: ProgressBar,
) {
    let mut futures = vec![];
    let ctx = CrawlCtx {
        nodes,
        result,
        count,
        db,
        http_client,
        instance_bar,
        video_bar,
    };
    for instance in instances {
        let f = fetch(instance, ctx.clone());
        futures.push(f);
    }
    join_all(futures).await;
}

async fn fetch_video(name: String, http_client: Arc<HttpClient>, video_bar: ProgressBar) {
    let mut videos_to_fetch: u64 = 1;
    let mut fetched_total: bool = false;
    let mut index: u64 = 0;
    let instance_url = "https://".to_owned() + name.clone().as_str();
    let filename = OUTPUT_DIR.to_owned() + &name + ".json";
    let mut videos: Vec<Video> = vec![];
    while index < videos_to_fetch {
        let query_videos = instance_url.clone()
            + "/api/v1/videos?count="
            + &MAX_VIDEOS.to_string()
            + "&filter=local"
            + "&start="
            + &index.to_string();
        video_bar.tick();
        let request = Request::get(&query_videos).body(()).unwrap();
        match http_client.send_async(request).await {
            Ok(mut resp) => match resp.json::<serde_json::Value>() {
                Ok(json) => {
                    if let Some(data) = json["data"].as_array() {
                        index += data.len() as u64;
                        if let Some(total) = json["total"].as_u64() {
                            if !fetched_total {
                                videos_to_fetch = total;
                                fetched_total = true;
                                video_bar.inc_length(videos_to_fetch);
                            }
                        }
                        video_bar.inc(data.len() as u64);
                        for value in data.iter() {
                            match serde_json::from_value(value.clone()) {
                                Ok(video) => videos.push(video),
                                Err(e) => {
                                    trace!(
                                        "Failed to parse peertube response from {}: {}",
                                        name,
                                        e
                                    );
                                }
                            }
                        }
                    } else {
                        error!(
                            "{}",
                            format!("[{}][{}] - JSON : {:?}", name, "/videos/", json).as_str()
                        );
                        break;
                    }
                    /*let database = Database::default();
                    process_videos(database, result);*/
                }
                Err(e) => {
                    trace!(
                        "Invalid json from {} : {} \nJson : \n{}\n----\n",
                        name,
                        e,
                        resp.text().unwrap_or("Invalid body".to_string())
                    );
                    break;
                }
            },
            Err(e) => {
                trace!("Failed to fetch videos from {} : {}", query_videos, e);
                break;
            }
        }
    }
    info!(
        "[{}][{}] Fetch complete ({} videos)",
        name, "/videos/", index
    );
    if !videos.is_empty() {
        write_to_file(filename, videos).await;
    }
}

async fn fetch_follow(
    api_endpoint: &'static str,
    entry_name: &'static str,
    name: String,
    ctx: CrawlCtx,
    instance: Arc<Mutex<APIInstance>>,
) {
    let mut tasks = Vec::new();
    let mut followers_to_fetch: u64 = 1;
    let mut index: u64 = 0;
    let mut fetched_total: bool = false;
    while index < followers_to_fetch {
        let query = "https://".to_owned()
            + name.clone().as_str()
            + "/api/v1"
            + api_endpoint
            + "?count=100"
            + "&start="
            + &index.to_string();
        ctx.instance_bar.tick();
        let request = Request::get(&query).body(()).unwrap();
        match ctx.http_client.send_async(request).await {
            Ok(mut req) => match req.json::<serde_json::Value>() {
                Ok(json) => {
                    if let Some(total) = json["total"].as_u64() {
                        if !fetched_total {
                            followers_to_fetch = total;
                            fetched_total = true;
                            ctx.instance_bar.inc_length(total);
                        }
                    } else if !fetched_total {
                        followers_to_fetch = 0;
                        fetched_total = true;
                    }
                    match json["data"].as_array() {
                        Some(data) => {
                            index += data.len() as u64;
                            ctx.instance_bar.inc(data.len() as u64);
                            for entry in data {
                                if let Some(hostname) = entry[entry_name]["host"].as_str() {
                                    if hostname != name {
                                        tasks.push(
                                            queue_for_crawling(hostname.to_string(), ctx.clone())
                                                .await,
                                        );
                                        instance.lock().await.followers.push(hostname.to_owned());
                                    }
                                }
                            }
                        }
                        None => {
                            error!(
                                "{}",
                                format!(
                                    "[{}][{}] - Non spec compliant json : {:?}",
                                    name, api_endpoint, json
                                )
                                .as_str()
                            );
                            break;
                        }
                    }
                }
                Err(e) => {
                    trace!("[{}][{}] Failed to parse json : {} ", name, api_endpoint, e);
                    break;
                }
            },
            Err(e) => {
                match e {
                    isahc::Error::ConnectFailed
                    | isahc::Error::BadServerCertificate(_)
                    | isahc::Error::SSLConnectFailed(_)
                    | isahc::Error::CouldntResolveHost => {}
                    _ => trace!("[{}][{}] Failed : {}", name, api_endpoint, e),
                };
                break;
            }
        }
    }
    info!(
        "[{}][{}] Fetch complete ({}/{})",
        name,
        api_endpoint,
        index,
        if fetched_total {
            followers_to_fetch.to_string()
        } else {
            "?".to_string()
        }
    );
    join_all(tasks).await;
}

async fn fetch(name: String, ctx: CrawlCtx) {
    let instance = Arc::new(Mutex::new(APIInstance::new(name.clone())));

    // Request ressources from host
    let t0 = fetch_follow(
        "/server/following",
        "following",
        name.clone(),
        ctx.clone(),
        instance.clone(),
    );

    let t1 = fetch_follow(
        "/server/followers",
        "follower",
        name.clone(),
        ctx.clone(),
        instance.clone(),
    );

    let t2 = fetch_video(name.clone(), ctx.http_client, ctx.video_bar);

    join3(t0, t1, t2).await;
    ctx.instance_bar.inc(1);
    trace!("[{}] Done", name);
}

fn create_output_folder() {
    if !Path::new("./crawled").exists() {
        std::fs::create_dir("./crawled").expect("Failed to create output dir");
    }
}

fn display_cli(mb: Arc<MultiProgress>) {
    mb.join().unwrap();
}

async fn crawl(root: Option<String>) {
    // TODO : use SegQueue
    let nodes = Arc::new(Mutex::new(HashSet::new()));
    let result: Arc<Mutex<HashSet<APIInstance>>> = Arc::new(Mutex::new(HashSet::new()));
    let count = Arc::new(Mutex::new(0));
    create_output_folder();
    let mb = Arc::new(MultiProgress::new());
    let instance_bar = mb.add(ProgressBar::new(0));
    let video_bar = mb.add(ProgressBar::new(0));
    video_bar.set_prefix("Fetching videos :");
    instance_bar.set_prefix("Fetching neighbours :");
    let sty = ProgressStyle::default_bar()
        .template("{prefix} [{wide_bar:.cyan/blue}] {per_sec} {pos:>7}/{len:7}({percent}%) {eta} remaining")
        .progress_chars("=>-");
    instance_bar.set_style(sty.clone());
    video_bar.set_style(sty.clone());
    video_bar.tick();
    instance_bar.tick();
    let mb_clone = mb.clone();

    // Handle startup logic : either we received a root to start from, or we fetch joinpeertube.org
    let mut instances = vec![];
    let instance_db = Arc::new(Mutex::new(InstanceDb::new()));
    if let Some(instance) = root {
        instances.push(instance);
    } else {
        instances = instance_db.lock().await.get_all_instances();
    }
    for instance in &instances[0..min(LIMIT as usize, instances.len())] {
        nodes.lock().await.insert(instance.clone());
    }
    instance_bar.set_length(instances.len().try_into().unwrap());
    instance_bar.println(format!("Loaded {} instances", instances.len()));
    if nodes.lock().await.len() == 0 {
        info!("No instance found in the database, seeding from https://instances.joinpeertube.org");
        match fetch_instance_list_from_joinpeertube() {
            Ok(res) => {
                instance_bar.set_length(res.len().try_into().unwrap());
                instance_bar.println(format!("Fetched {} instances", res.len()));
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
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(10))
        .connection_cache_size(4096 * 100_000_000) /* 100 MB cache */
        .build()
        .unwrap();

    std::thread::spawn(move || {
        display_cli(mb_clone);
    });

    crawl_from_instances(
        instances,
        nodes.clone(),
        result.clone(),
        count,
        instance_db.clone(),
        Arc::new(client),
        instance_bar.clone(),
        video_bar.clone(),
    )
    .await;
    instance_bar.finish_with_message(&format!("Found {} instances", nodes.lock().await.len()));
    video_bar.finish_with_message("Fetched all videos");
    let duration = start.elapsed();
    info!(
        "Added {} instances in {} seconds",
        (*nodes.lock().await).len(),
        duration.as_secs()
    );
}

fn elastic_is_online(client: HttpClient) -> bool {
    if let Err(e) = create_mappings("http://localhost:9200".to_string(), client) {
        error!("{}", e);
        false
    } else {
        info!("Sucessfully initialized elastic search");
        true
    }
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

    /// Root domain name
    /// Uses joinpeertube.org if missing
    #[structopt(short = "r", long = "root")]
    root: Option<String>,
}

fn main() -> Result<(), ()> {
    let opt = Opt::from_args();
    let client = isahc::HttpClient::new().unwrap();
    stderrlog::new()
        .module(module_path!())
        .quiet(opt.quiet)
        .verbosity(opt.verbose)
        .timestamp(opt.ts.unwrap_or(stderrlog::Timestamp::Off))
        .color(ColorChoice::Always)
        .init()
        .unwrap();
    info!("Starting crawler");
    if elastic_is_online(client) {
        block_on(crawl(opt.root));
        Ok(())
    } else {
        error!("Failed to connect to elastic instance");
        Err(())
    }
}
