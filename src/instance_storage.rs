use log::warn;
use rusqlite::{Connection, NO_PARAMS};
pub struct InstanceDb {
    conn: Connection,
    new_instance_inserted: u32,
}

impl Default for InstanceDb {
    fn default() -> Self {
        Self::new()
    }
}

impl InstanceDb {
    pub fn new() -> InstanceDb {
        let conn = Connection::open("instances.db").expect("Failed to open DB");
        conn.execute(
            "create table if not exists peertube_instances (
             id integer primary key,
             base_url text not null unique
         )",
            NO_PARAMS,
        )
        .expect("Failed to create table");
        InstanceDb {
            conn,
            new_instance_inserted: 0,
        }
    }

    pub fn insert_instance(&mut self, instance: String) {
        match self.conn.execute(
            "insert or ignore into peertube_instances (base_url) values (?1)",
            &[instance],
        ) {
            Ok(_) => (),
            Err(e) => warn!("Failed to insert instance into database : {}", e),
        }
    }

    pub fn get_all_instances(&self) -> Vec<String> {
        let mut stmt = self
            .conn
            .prepare("select base_url from peertube_instances")
            .unwrap();
        let instance_iter = stmt
            .query_map(NO_PARAMS, |row| Ok(row.get(0).unwrap()))
            .unwrap();
        instance_iter
            .filter_map(Result::ok)
            .collect::<Vec<String>>()
    }

    pub fn get_instance_added(&self) -> u32 {
        self.new_instance_inserted
    }
}
