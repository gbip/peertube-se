use crate::video::Video;
use elastic::client::requests::common::Doc;
use elastic::client::requests::{
    DeleteRequestBuilder, GetRequestBuilder, IndexRequestBuilder, UpdateRequestBuilder,
};
use elastic::client::{AsyncClientBuilder, Client};
use elastic::http::sender::AsyncSender;

use futures::future::{ok, Future};

type DatabaseSender = AsyncSender;
type DatabaseClient = Client<DatabaseSender>;
use std::sync::Arc;

pub struct Database {
    elastic_client: DatabaseClient,
}

impl Default for Database {
    fn default() -> Database {
        let elastic_client = AsyncClientBuilder::new()
            .build()
            .expect("Failed to create elastic client");
        Database { elastic_client }
    }
}

impl Database {
    pub fn new(elastic_client: DatabaseClient) -> Database {
        Database { elastic_client }
    }

    pub fn video_is_present(&self, video: &Video) -> GetRequestBuilder<DatabaseSender, Video> {
        self.elastic_client
            .document::<Video>()
            .get(video.uuid.clone())
    }

    pub fn delete_video(&self, video: &Video) -> DeleteRequestBuilder<DatabaseSender, Video> {
        self.elastic_client
            .document::<Video>()
            .delete(video.uuid.clone())
    }

    pub fn update_video(&self, video: &Video) -> UpdateRequestBuilder<DatabaseSender, Doc<Video>> {
        self.elastic_client
            .document::<Video>()
            .update(video.uuid.clone())
            .doc(video.clone())
    }

    pub fn get_video(&self, video: &Video) -> GetRequestBuilder<DatabaseSender, Video> {
        self.elastic_client
            .document::<Video>()
            .get(video.uuid.clone())
    }

    pub fn index_video(&self, video: &Video) -> IndexRequestBuilder<DatabaseSender, Video> {
        self.elastic_client.document::<Video>().index(video.clone())
    }
}

fn process_video(db: Arc<Database>, video: Arc<Video>) -> impl Future<Item = (), Error = ()> {
    let db_handle = db.clone();
    let video_handle = video.clone();
    let future = db_handle
        .update_video(&video)
        .send()
        .and_then(|resp| {
            //println!("Updated ? :{}", resp.updated());
            if !resp.updated() {
                println!("Video not updated");
            }
            ok(())
        })
        .map_err(move |e| {
            tokio::spawn(
                db.index_video(&video_handle)
                    .send()
                    .and_then(|_| {
                        println!("Video indexed !");
                        ok(())
                    })
                    .map_err(|e| println!("Indexing video : {}", e)),
            );
            println!("Failed to update video : {}", e)
        });
    Box::new(future)
}

pub fn process_videos(db: Database, videos: Vec<Video>) {
    let db_handle = Arc::new(db);
    for video in videos {
        let video_handle = Arc::new(video);
        let future = process_video(db_handle.clone(), video_handle);;
        tokio::spawn(future);
    }
}
