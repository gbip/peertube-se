#![feature(proc_macro_hygiene)]
#![feature(decl_macro)]

use elastic::client::SyncClientBuilder;
use peertube_lib::video::Video;
use rocket::http::RawStr;
use rocket::{get, routes};
use rocket_contrib::templates::Template;
use serde_json::json;

#[get("/search?<query>")]
fn index(query: &RawStr) -> Template {
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
            if let Some(hit) = response.hits().nth(0) {
                Template::render("query_result", hit.document().unwrap())
            } else {
                Template::render("error.hbs", format!("No matches found for query {}", query))
            }
        }
        Err(e) => Template::render("error", format!("Error : {}", e)),
    }
}

fn main() {
    rocket::ignite()
        .attach(Template::fairing())
        .mount("/", routes![index])
        .launch();
}
