use crate::db::{process_videos, Database};

use futures::future::lazy;
use futures::future::ok;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};

#[derive(Serialize, Deserialize, ElasticType, Clone)]
pub struct Video {
    pub description: String,
    pub name: String,
    #[elastic(id(expr = "uuid"))]
    pub uuid: String,
    pub views: i64,
    pub likes: i64,
    pub duration: i64,
    #[serde(rename(serialize = "createdAt", deserialize = "createdAt"))]
    pub created_at: String,
    pub thumbnail: String,
    pub creator: String,
    pub nsfw: bool,
}

impl Video {
    pub fn update_db_from_file(filename: String) {
        let file = File::open(filename.clone()).expect(&format!("Failed to open {}", filename));
        let reader = BufReader::new(file);
        let mut videos = Vec::new();
        for line in reader.lines() {
            let video: Video = serde_json::from_str(&line.unwrap()).unwrap();
            videos.push(video);
        }
        let database = Database::default();
        tokio::run(lazy(|| {
            process_videos(database, videos);
            ok(())
        }));
    }
}
