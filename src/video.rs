use elastic::client::SyncClientBuilder;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Serialize, Deserialize, ElasticType, Clone)]
pub struct Video {
    pub description: String,
    pub name: String,
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
        let file = File::open(filename).expect("Failed to open file");
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let video: Video = serde_json::from_str(&line.unwrap()).unwrap();
            println!("Sending {} to ES", video.name);
            let elastic_client = SyncClientBuilder::new()
                .build()
                .expect("Failed to create elastic client");
            elastic_client.document().index(video).send().unwrap();
        }
    }
}
