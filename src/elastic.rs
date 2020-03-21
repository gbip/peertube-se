use isahc::{HttpClient, ResponseExt};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read};

enum Index {
    IndexIsPresent,
    IndexIsMissing,
}

fn index_exist(es_addr: String, client: &HttpClient) -> Result<Index, Box<dyn Error>> {
    let mut resp = client.get(es_addr + "/peertube_se")?;
    let mut result = Ok(Index::IndexIsPresent);
    if let Ok(json) = resp.json::<serde_json::Value>() {
        if let Some(code) = json["code"].as_u64() {
            if code == 404 {
                result = Ok(Index::IndexIsMissing);
            }
        }
    }
    result
}

/// Creates the elastic search mapping for Peertube videos
pub fn create_mappings(es_addr: String, client: HttpClient) -> Result<(), Box<dyn Error>> {
    if let Index::IndexIsMissing = index_exist(es_addr.clone(), &client)? {
        let file = File::open(&es_addr)?;
        let mut buf_reader = BufReader::new(file);
        let mut mappings = String::new();
        buf_reader.read_to_string(&mut mappings)?;

        // Test me with curl :
        // `curl -X PUT localhost:9200/mapping_test2 -d "$(cat es_mappings.json)" -H "Content-Type: application/json`
        let mut resp = client.put(es_addr + "/peertube_se", mappings)?;
        if let Ok(json) = resp.json::<serde_json::Value>() {
            // The expected answer is :
            // {
            //   "acknowledged": true,
            //   "shards_acknowledged": true,
            //   "index": "mapping_test"
            // }
            if let Some(acknowledged) = json["acknowledged"].as_bool() {
                if !acknowledged {
                    return Err(
                        format!("Elastic Search failed to create mapping : {}", json).into(),
                    );
                }
            }
        }
    }
    Ok(())
}
