#![allow(unused_imports)]
use futures::Future;
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
use std::time::Duration;
use tokio;
use tokio::prelude::future::lazy;
use tokio::prelude::future::ok;
use tokio::prelude::AsyncWrite;

use elastic::client::AsyncClientBuilder;
use elastic::AsyncClient;

use peertube_lib::db;
use peertube_lib::video::Video;

const FIRST: &str = "gouttedeau.space";

const URL_TO_TRY: [&str; 2] = ["/server/following", "/server/followers"];

const OUTPUT_DIR: &str = "crawled/";

const LIMIT: u8 = 10;

#[derive(Debug, Hash, Clone, Serialize, Deserialize)]
struct Instance {
    name: String,
    followers: Vec<String>,
    following: Vec<String>,
}

impl Instance {
    pub fn new(name: String) -> Instance {
        Instance {
            name: name,
            followers: vec![],
            following: vec![],
        }
    }
}

impl PartialEq for Instance {
    fn eq(&self, other: &Instance) -> bool {
        self.name == other.name
    }
}

impl Eq for Instance {}

fn process(
    item: String,
    nodes: &Arc<Mutex<HashSet<String>>>,
    result: &Arc<Mutex<HashSet<Instance>>>,
    count: Arc<Mutex<u8>>,
) {
    if !nodes.lock().unwrap().contains(&item) {
        //println!("Added {}", &item);
        nodes.lock().expect("Poison").insert(item.clone());
        let nodes_clone = nodes.clone();
        let result_clone = result.clone();
        if *(count.lock().unwrap()) < LIMIT {
            *(count.lock().unwrap()) += 1;
            tokio::spawn(fetch(item, nodes_clone, result_clone, count));
        }
    }
}

fn write_to_file(filename: String, data: Vec<Video>) -> impl Future<Error = ()> {
    println!("Opening {}", &filename);
    let open_file = tokio::fs::File::create(filename.clone())
        .map_err(|e| {
            println!("{:?}", e);
            e
        })
        .and_then(move |mut file| {
            let lines: String = data.into_iter().fold("".to_string(), |mut acc, o| {
                acc += &to_string(&o).unwrap();
                acc += "\n";
                acc
            });
            println!("Writing to {}", filename);
            file.poll_write(&lines.as_bytes())
        })
        .map_err(|e| println!("{:?}", e))
        .and_then(|_| ok(()));
    open_file
}

fn fetch(
    name: String,
    nodes: Arc<Mutex<HashSet<String>>>,
    result: Arc<Mutex<HashSet<Instance>>>,
    count: Arc<Mutex<u8>>,
) -> impl Future<Item = (), Error = ()> {
    lazy(move || {
        println!("Processing : {}", name);
        let instance = Arc::new(Mutex::new(Instance::new(name.clone())));
        // Request ressources from host
        let mut tasks = Vec::new();
        let base = "https://".to_owned() + name.clone().as_str() + "/api/v1";
        for url in &URL_TO_TRY {
            let count_local = count.clone();
            let instance_local = instance.clone();
            let instance_local2 = instance.clone();
            let nodes_local = nodes.clone();
            let result_local = result.clone();
            let name_local = name.clone();
            let query = base.clone() + url;
            let task = ClientBuilder::new()
                .timeout(Duration::new(5, 0))
                .build()
                .unwrap()
                .get(&query)
                .send()
                .and_then(|mut body| body.json::<serde_json::Value>())
                .map(move |res| {
                    for entry in res["data"].as_array().expect("Invalid json") {
                        if let Some(hostname) = entry["follower"]["host"].as_str() {
                            if hostname != name_local {
                                process(
                                    hostname.to_string(),
                                    &nodes_local,
                                    &result_local,
                                    count_local.clone(),
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
                                );
                            }
                        }
                    }
                    instance_local2
                })
                .map_err(move |e| {
                    println!("Failed to fetch {} ", e);
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
                .map_err(move |e| println!("Error while fetching {} : {}", query_videos.clone(), e))
                .and_then(move |json| {
                    let mut result = vec![];
                    for value in json["data"].as_array().unwrap() {
                        let thumbnail_uri = value["thumbnailPath"].to_string();
                        // Remove extra braces
                        let thumbnail =
                            instance_url.clone() + &thumbnail_uri[1..thumbnail_uri.len() - 1];
                        result.push(Video {
                            description: value["description"].to_string(),
                            name: value["name"].to_string(),
                            uuid: value["uuid"].to_string(),
                            views: value["views"].as_i64().unwrap_or(0),
                            likes: value["likes"].as_i64().unwrap_or(0),
                            duration: value["duration"].as_i64().unwrap_or(0),
                            created_at: value["createdAt"].to_string(),
                            creator: value["account"]["name"].to_string() + "@" + &name,
                            thumbnail,
                            nsfw: value["nsfw"].as_bool().unwrap_or(false),
                        });
                    }
                    ok(result)
                })
                .and_then(|data| {
                    db::add_videos_to_db(data.clone());
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
    // TODO : use SegQueue
    let nodes = Arc::new(Mutex::new(HashSet::new()));
    let work = Arc::new(Mutex::new(VecDeque::new()));
    let result: Arc<Mutex<HashSet<Instance>>> = Arc::new(Mutex::new(HashSet::new()));
    let count = Arc::new(Mutex::new(0));
    work.lock().unwrap().push_back(FIRST.to_string());
    nodes.lock().unwrap().insert(FIRST.to_string());
    tokio::run(fetch(
        FIRST.to_string(),
        nodes.clone(),
        result.clone(),
        count,
    ));
    println!("Found {} instances", nodes.lock().unwrap().len());
}
