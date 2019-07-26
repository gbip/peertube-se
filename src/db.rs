use crate::video::Video;
use elastic::client::AsyncClientBuilder;
use futures::future::Future;

pub fn add_videos_to_db(videos: Vec<Video>) {
    let elastic_client_builder = AsyncClientBuilder::new();
    let elastic_client = elastic_client_builder
        .build()
        .expect("Failed to create elastic client");
    for video in videos {
        tokio::spawn(
            elastic_client
                .document()
                .index(video)
                .send()
                .map_err(|e| {
                    println!("Failed to insert document in ES : {}", e);
                })
                .map(|_| {}),
        );
    }
}
