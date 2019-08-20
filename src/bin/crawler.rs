#![allow(unused_imports)]
use futures::future::join_all;
use futures::Future;
use log::*;
use petgraph::dot::Dot;
use petgraph::prelude::NodeIndex;
use petgraph::{Graph, Undirected};
use petgraph_graphml::GraphMl;
use reqwest;
use reqwest::r#async::ClientBuilder;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::to_string_pretty;
use serde_json::{from_reader, to_string};
use std::collections::vec_deque::VecDeque;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio;
use tokio::prelude::future::lazy;
use tokio::prelude::future::ok;
use tokio::prelude::AsyncWrite;

use elastic::client::AsyncClientBuilder;
use elastic::AsyncClient;

use env_logger;
use peertube_lib::instance_storage::InstanceDb;
use peertube_lib::peertube_api::fetch_instance_list_from_joinpeertube;
use peertube_lib::peertube_api::Video;
use peertube_lib::video_storage;
use peertube_lib::video_storage::process_videos;
use peertube_lib::video_storage::Database;

const URL_TO_TRY: [&str; 2] = ["/server/following", "/server/followers"];

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

fn process(
    item: String,
    nodes: &Arc<Mutex<HashSet<String>>>,
    result: &Arc<Mutex<HashSet<APIInstance>>>,
    count: Arc<Mutex<u64>>,
    db: Arc<Mutex<InstanceDb>>,
) {
    db.lock().unwrap().insert_instance(item.clone());
    if !nodes.lock().unwrap().contains(&item) {
        nodes.lock().expect("Poison").insert(item.clone());
        let nodes_clone = nodes.clone();
        let result_clone = result.clone();
        if *(count.lock().unwrap()) < LIMIT || LIMIT == 0 {
            *(count.lock().unwrap()) += 1;
            tokio::spawn(fetch(item, nodes_clone, result_clone, count, db));
        }
    }
}

fn write_to_file(filename: String, data: Vec<Video>) -> impl Future<Error = ()> {
    let open_file = tokio::fs::File::create(filename.clone())
        .map_err(|e| {
            warn!("Failed to open file {:?}", e);
            e
        })
        .and_then(move |mut file| {
            let lines: String = data.into_iter().fold("".to_string(), |mut acc, o| {
                acc += &to_string(&o).unwrap();
                acc += "\n";
                acc
            });
            file.poll_write(&lines.as_bytes())
        })
        .map_err(|e| warn!("Failed to write to file : {:?}", e))
        .and_then(|_| ok(()));
    open_file
}

fn fetch_all(
    instances: Vec<String>,
    nodes: Arc<Mutex<HashSet<String>>>,
    result: Arc<Mutex<HashSet<APIInstance>>>,
    count: Arc<Mutex<u64>>,
    db: Arc<Mutex<InstanceDb>>,
) -> impl Future<Item = (), Error = ()> {
    let mut futures = vec![];
    for instance in instances {
        let f = fetch(
            instance,
            nodes.clone(),
            result.clone(),
            count.clone(),
            db.clone(),
        );
        futures.push(f);
    }
    let future = join_all(futures);
    future.map(|_| ())
}

fn fetch(
    name: String,
    nodes: Arc<Mutex<HashSet<String>>>,
    result: Arc<Mutex<HashSet<APIInstance>>>,
    count: Arc<Mutex<u64>>,
    db: Arc<Mutex<InstanceDb>>,
) -> impl Future<Item = (), Error = ()> {
    lazy(move || {
        trace!("Processing instance : {}", name);
        let instance = Arc::new(Mutex::new(APIInstance::new(name.clone())));
        // Request ressources from host
        let mut tasks = Vec::new();
        let base = "https://".to_owned() + name.clone().as_str() + "/api/v1";
        trace!("Queryig : {}", base);
        for url in &URL_TO_TRY {
            let count_local = count.clone();
            let instance_local = instance.clone();
            let instance_local2 = instance.clone();
            let nodes_local = nodes.clone();
            let result_local = result.clone();
            let name_local = name.clone();
            let db_local = db.clone();
            let query = base.clone() + url;
            let task = ClientBuilder::new()
                .timeout(Duration::new(5, 0))
                .build()
                .unwrap()
                .get(&query)
                .send()
                .and_then(|mut body| body.json::<serde_json::Value>())
                .map(move |res| {
                    if let Some(data) = res["data"].as_array() {
                        for entry in data {
                            if let Some(hostname) = entry["follower"]["host"].as_str() {
                                if hostname != name_local {
                                    process(
                                        hostname.to_string(),
                                        &nodes_local,
                                        &result_local,
                                        count_local.clone(),
                                        db_local.clone(),
                                    );
                                    instance_local
                                        .lock()
                                        .unwrap()
                                        .followers
                                        .push(hostname.to_owned());
                                }
                            }
                            if let Some(hostname) = entry["following"]["host"].as_str() {
                                if hostname != name_local {
                                    instance_local
                                        .lock()
                                        .unwrap()
                                        .following
                                        .push(hostname.to_owned());
                                    process(
                                        hostname.to_string(),
                                        &nodes_local,
                                        &result_local,
                                        count_local.clone(),
                                        db_local.clone(),
                                    );
                                }
                            }
                        }
                    }
                    instance_local2
                })
                .map_err(move |e| {
                    trace!("Failed to fetch instance : {} ", e);
                });
            tasks.push(task);
        }
        {
            let instance_url = "https://".to_owned() + name.clone().as_str();
            let query_videos = instance_url.clone() + "/api/v1/videos";
            let filename = OUTPUT_DIR.to_owned() + &name + ".json";
            let task = ClientBuilder::new()
                .timeout(Duration::new(5, 0))
                .build()
                .unwrap()
                .get(&query_videos)
                .send()
                .and_then(|mut body| body.json::<serde_json::Value>())
                .map_err(move |e| {
                    trace!("Error while fetching url {} : {}", query_videos.clone(), e)
                })
                .and_then(move |json| {
                    let mut result = vec![];
                    if let Some(data) = json["data"].as_array() {
                        for value in data.into_iter() {
                            match serde_json::from_value(value.clone()) {
                                Ok(video) => result.push(video),
                                Err(e) => trace!("Failed to parse peertube response : {}", e),
                            }
                        }
                    }
                    ok(result)
                })
                .and_then(|data| {
                    let database = Database::default();
                    process_videos(database, data.clone());
                    write_to_file(filename, data)
                })
                .map_err(|_| ())
                .map(|_| ());
            tokio::spawn(task);
        }
        let mut iter = tasks.into_iter();
        let t0 = iter.next();
        let t1 = iter.next();
        //let p = tasks.into_iter().fold(lazy(||ok(())), |future, acc| future.join(acc));
        //let (t0, t1) = (tasks[0], tasks[1]);

        let f = t0.join(t1).and_then(move |(val, _)| {
            result
                .lock()
                .unwrap()
                .insert(val.unwrap().lock().unwrap().clone());
            ok(())
        });
        tokio::spawn(f);
        ok(())
    })
}

fn main() {
    env_logger::init();

    // TODO : use SegQueue
    let nodes = Arc::new(Mutex::new(HashSet::new()));
    let result: Arc<Mutex<HashSet<APIInstance>>> = Arc::new(Mutex::new(HashSet::new()));
    let count = Arc::new(Mutex::new(0));

    let instance_db = Arc::new(Mutex::new(InstanceDb::new()));
    let mut instances = instance_db.lock().unwrap().get_all_instances();

    for instance in &instances {
        nodes.lock().unwrap().insert(instance.clone());
    }

    if nodes.lock().unwrap().len() == 0 {
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
    tokio::run(fetch_all(
        instances,
        nodes.clone(),
        result.clone(),
        count,
        instance_db.clone(),
    ));
    let duration = start.elapsed();
    info!("Found {} instances", nodes.lock().unwrap().len());
    info!(
        "Added {} instances",
        instance_db.lock().unwrap().get_instance_added()
    );
    info!("In {} sec", duration.as_secs());
}
