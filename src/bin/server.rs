#![feature(proc_macro_hygiene)]
#![feature(decl_macro)]

use elastic::client::SyncClientBuilder;
use peertube_lib::video::Video;
use rocket::http::RawStr;
use rocket::{get, routes};
use rocket_contrib::templates::Template;
use serde::Serialize;
use serde_json::json;

#[derive(Serialize)]
struct VideoTemplate {
    videos: Vec<Video>,
    parent: &'static str,
}

#[get("/index.html")]
fn index() -> &'static str {
    "Welcome to the Peertube search engine"
}

#[get("/search?<query>")]
fn search(query: &RawStr) -> Template {
    let elastic_client = SyncClientBuilder::new().build().unwrap();
    let query = json!({
        "query": {
          "multi_match": {
             "query" : query.to_string(),
              "fields": ["description", "name"]
          }
        }
    });
    let result = elastic_client
        .document::<Video>()
        .search()
        .body(query.clone())
        .send();
    match result {
        Ok(response) => {
            println!("Found {} videos", response.total());
            let videos = response.into_documents().collect::<Vec<Video>>();
            if !videos.is_empty() {
                let context = VideoTemplate {
                    parent: "layout",
                    videos,
                };
                Template::render("video", context)
            } else {
                Template::render("error", format!("No matches found for query {}", query))
            }
        }
        Err(e) => Template::render("error", format!("Error : {}", e)),
    }
}

fn main() {
    rocket::ignite()
        .attach(Template::fairing())
        .mount("/", routes![index, search])
        .launch();
}
