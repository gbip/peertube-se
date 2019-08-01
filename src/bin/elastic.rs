use peertube_lib::video::Video;

fn main() {
    Video::update_db_from_file("crawled/gouttedeau.space.json".to_string());
}
