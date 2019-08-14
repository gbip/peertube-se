use rusqlite::{Connection, NO_PARAMS};

#[derive(Debug, Hash)]
pub struct Instance {
    pub base_url: String,
    pub blacklisted: bool,
}

pub struct InstanceDb {
    conn: Connection,
    new_instance_inserted: u32,
}

impl InstanceDb {
    pub fn new() -> InstanceDb {
        let conn = Connection::open("instances.db").expect("Failed to open DB");
        conn.execute(
            "create table if not exists peertube_instances (
             id integer primary key,
             base_url text not null unique,
             blacklisted boolean not null
         )",
            NO_PARAMS,
        )
        .expect("Failed to create table");
        InstanceDb {
            conn,
            new_instance_inserted: 0,
        }
    }

    pub fn insert_instance(&mut self, instance: Instance) {
        match self.conn.execute(
            "insert or ignore into peertube_instances (base_url, blacklisted) values (?1, ?2)",
            &[instance.base_url, instance.blacklisted.to_string()],
        ) {
            Ok(_) => self.new_instance_inserted += 1,
            Err(_) => (),
        }
    }

    pub fn get_all_instances(&self) -> Vec<Instance> {
        let mut stmt = self
            .conn
            .prepare("select base_url, blacklisted from peertube_instances")
            .unwrap();
        let instance_iter = stmt
            .query_map(NO_PARAMS, |row| {
                Ok(Instance {
                    base_url: row.get(0).unwrap(),
                    blacklisted: false,
                })
            })
            .unwrap();
        instance_iter
            .filter_map(Result::ok)
            .collect::<Vec<Instance>>()
    }

    pub fn get_instance_added(&self) -> u32 {
        self.new_instance_inserted
    }
}
