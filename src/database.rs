
use mongodb::{
    Client, Database, options::ClientOptions, options::StreamAddress
};


/* 戦歴を管理するクラス */
struct BattleHistory {
    client: Client,
    database: Database,
}
static DATABASE_NAME: &str = "smabrog_battle_history";
impl BattleHistory {
    fn new() -> BattleHistory {
        // MongoDBへの接続(の代わりに作成)とdatabaseの取得
        let options = ClientOptions::builder()
            .hosts(vec![
                StreamAddress {
                    hostname: "localhost".into(),
                    port: Some(27017),
                }
            ])
            .build();
        let client = Client::with_options(options).unwrap();
        let database = client.database(DATABASE_NAME);

        BattleHistory {
            client: client,
            database: database,
        }
    }
}
