use std::sync::{Arc, Weak};

use dashmap::DashMap;

use crate::db::{Db, UserIdInDb};

// use this to maintain Storage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StorageToken {
    pub token: String,
    pub client_url: url::Url,
    pub machine_id: uuid::Uuid,
    pub user_id: UserIdInDb,
}

#[derive(Debug, Clone)]
struct ClientInfo {
    storage_token: StorageToken,
    report_time: i64,
    running_network: bool,
}

#[derive(Debug)]
pub struct StorageInner {
    user_clients_map: DashMap<UserIdInDb, DashMap<uuid::Uuid, ClientInfo>>,
    pub db: Db,
}

#[derive(Debug, Clone)]
pub struct Storage(Arc<StorageInner>);
pub type WeakRefStorage = Weak<StorageInner>;

impl TryFrom<WeakRefStorage> for Storage {
    type Error = ();

    fn try_from(weak: Weak<StorageInner>) -> Result<Self, Self::Error> {
        weak.upgrade().map(Storage).ok_or(())
    }
}

impl From<Arc<StorageInner>> for Storage {
    fn from(inner: Arc<StorageInner>) -> Self {
        Storage(inner)
    }
}

impl Storage {
    pub fn new(db: Db) -> Self {
        Storage(Arc::new(StorageInner {
            user_clients_map: DashMap::new(),
            db,
        }))
    }

    fn remove_client_info_map(map: &DashMap<uuid::Uuid, ClientInfo>, stoken: &StorageToken) {
        map.remove_if(&stoken.machine_id, |_, v| {
            v.storage_token.client_url == stoken.client_url
                && v.storage_token.user_id == stoken.user_id
        });
    }

    fn update_client_info_map(map: &DashMap<uuid::Uuid, ClientInfo>, client_info: &ClientInfo) {
        map.entry(client_info.storage_token.machine_id)
            .and_modify(|e| {
                if e.report_time < client_info.report_time {
                    assert_eq!(
                        e.storage_token.machine_id,
                        client_info.storage_token.machine_id
                    );
                    *e = client_info.clone();
                }
            })
            .or_insert(client_info.clone());
    }

    pub fn update_client(&self, stoken: StorageToken, report_time: i64) {
        self.update_client_with_running_network(stoken, report_time, false);
    }

    pub fn update_client_with_running_network(
        &self,
        stoken: StorageToken,
        report_time: i64,
        running_network: bool,
    ) {
        let inner = self.0.user_clients_map.entry(stoken.user_id).or_default();

        let client_info = ClientInfo {
            storage_token: stoken.clone(),
            report_time,
            running_network,
        };
        Self::update_client_info_map(&inner, &client_info);
    }

    /// 统计某用户当前运行网络的不同设备数，可排除指定设备（通常是提交心跳的设备自身）。
    pub fn count_running_network_clients(
        &self,
        user_id: UserIdInDb,
        exclude_machine_id: Option<uuid::Uuid>,
    ) -> usize {
        self.0
            .user_clients_map
            .get(&user_id)
            .map(|info_map| {
                info_map
                    .iter()
                    .filter(|entry| {
                        Some(*entry.key()) != exclude_machine_id && entry.value().running_network
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn remove_client(&self, stoken: &StorageToken) {
        self.0
            .user_clients_map
            .remove_if(&stoken.user_id, |_, set| {
                Self::remove_client_info_map(set, stoken);
                set.is_empty()
            });
    }

    pub fn weak_ref(&self) -> WeakRefStorage {
        Arc::downgrade(&self.0)
    }

    pub fn get_client_url_by_machine_id(
        &self,
        user_id: UserIdInDb,
        machine_id: &uuid::Uuid,
    ) -> Option<url::Url> {
        self.0.user_clients_map.get(&user_id).and_then(|info_map| {
            info_map
                .get(machine_id)
                .map(|info| info.storage_token.client_url.clone())
        })
    }

    pub fn list_user_clients(&self, user_id: UserIdInDb) -> Vec<url::Url> {
        self.0
            .user_clients_map
            .get(&user_id)
            .map(|info_map| {
                info_map
                    .iter()
                    .map(|info| info.value().storage_token.client_url.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn list_user_clients_with_machine_id(
        &self,
        user_id: UserIdInDb,
    ) -> Vec<(uuid::Uuid, url::Url)> {
        self.0
            .user_clients_map
            .get(&user_id)
            .map(|info_map| {
                info_map
                    .iter()
                    .map(|info| {
                        let info = info.value();
                        (
                            info.storage_token.machine_id,
                            info.storage_token.client_url.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn db(&self) -> &Db {
        &self.0.db
    }

    pub async fn auto_create_user(&self, username: &str) -> anyhow::Result<UserIdInDb> {
        let new_user = self.db().auto_create_user(username).await?;
        tracing::info!("Auto-created user '{}' with id {}", username, new_user.id);
        Ok(new_user.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_storage_token(
        user_id: UserIdInDb,
        machine_id: uuid::Uuid,
        client_url: &str,
    ) -> StorageToken {
        StorageToken {
            token: format!("token-{machine_id}"),
            client_url: client_url.parse().unwrap(),
            machine_id,
            user_id,
        }
    }

    #[tokio::test]
    async fn machine_id_is_scoped_within_each_user() {
        let storage = Storage::new(Db::memory_db().await);
        let machine_id = uuid::Uuid::new_v4();

        let user1_token = make_storage_token(1, machine_id, "tcp://127.0.0.1:1001");
        let user2_token = make_storage_token(2, machine_id, "tcp://127.0.0.1:1002");

        storage.update_client(user1_token.clone(), 10);
        storage.update_client(user2_token.clone(), 20);

        assert_eq!(
            storage.get_client_url_by_machine_id(1, &machine_id),
            Some(user1_token.client_url.clone())
        );
        assert_eq!(
            storage.get_client_url_by_machine_id(2, &machine_id),
            Some(user2_token.client_url.clone())
        );

        storage.remove_client(&user1_token);

        assert_eq!(storage.get_client_url_by_machine_id(1, &machine_id), None);
        assert_eq!(
            storage.get_client_url_by_machine_id(2, &machine_id),
            Some(user2_token.client_url.clone())
        );

        storage.remove_client(&user2_token);

        assert_eq!(storage.get_client_url_by_machine_id(2, &machine_id), None);
    }

    #[tokio::test]
    async fn count_running_network_clients_excludes_selected_machine() {
        let storage = Storage::new(Db::memory_db().await);
        let running_machine = uuid::Uuid::new_v4();
        let idle_machine = uuid::Uuid::new_v4();
        let candidate_machine = uuid::Uuid::new_v4();

        storage.update_client_with_running_network(
            make_storage_token(1, running_machine, "tcp://127.0.0.1:2001"),
            10,
            true,
        );
        storage.update_client_with_running_network(
            make_storage_token(1, idle_machine, "tcp://127.0.0.1:2002"),
            10,
            false,
        );
        storage.update_client_with_running_network(
            make_storage_token(1, candidate_machine, "tcp://127.0.0.1:2003"),
            10,
            false,
        );

        assert_eq!(storage.count_running_network_clients(1, None), 1);
        assert_eq!(
            storage.count_running_network_clients(1, Some(candidate_machine)),
            1
        );
        assert_eq!(
            storage.count_running_network_clients(1, Some(running_machine)),
            0
        );
    }
}
