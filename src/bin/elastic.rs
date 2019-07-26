use peertube_lib::video::Video;

fn main() {
    Video::update_db_from_file("peertube_crawler/crawled/gouttedeau.space.json".to_string());
}
