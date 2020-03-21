use peertube_lib::peertube_api::Video;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
const DIR: &str = "./crawled";

fn main() -> Result<(), Box<dyn Error>> {
    let mut data: HashMap<String, Video> = HashMap::new();
    println!("Starting inspection");
    let mut total = 0;
    let files = fs::read_dir(DIR)?;
    for file_res in files {
        if let Ok(file) = file_res {
            println!("Opening {}", file.path().to_str().unwrap());
            let input = File::open(file.path())?;
            let buffer = BufReader::new(input);
            let mut count = 0;
            for line in buffer.lines() {
                count += 1;
                let video: Video = serde_json::from_str(&line?)?;
                if data.contains_key(&video.uuid) {
                    let v = data.get(&video.uuid).unwrap();
                    println!(
                        "Found duplicate : {} ({})from {}@{} already exists ({} ({})from {}@{})",
                        video.name,
                        video.uuid,
                        video.account.display_name,
                        video.account.host,
                        v.name,
                        v.uuid,
                        v.account.display_name,
                        v.account.host
                    );
                } else {
                    data.insert(video.uuid.clone(), video);
                }
            }
            total += count;
            println!("Inspected {} videos", count)
        }
    }
    println!("There are {} videos in the BDD", total);
    Ok(())
}
