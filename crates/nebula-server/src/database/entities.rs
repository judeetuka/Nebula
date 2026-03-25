//! SeaORM entity definitions for NEBULA server persistent storage.
//!
//! These tables back the REST API and admin dashboard. The in-memory
//! `ClusterRegistry` remains the source of truth for live tunnel state;
//! these entities provide durable records for auditing, analytics, and
//! multi-server synchronisation.

/// ── clusters ───────────────────────────────────────────────────────────────
pub mod clusters {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "clusters")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub name: String,
        pub created_at: DateTimeUtc,
        pub max_nodes: i32,
        pub auth_token_hash: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::nodes::Entity")]
        Nodes,
    }

    impl Related<super::nodes::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Nodes.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

/// ── nodes ──────────────────────────────────────────────────────────────────
pub mod nodes {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "nodes")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub cluster_id: String,
        pub role: String,
        pub last_heartbeat: DateTimeUtc,
        pub battery_level: Option<i32>,
        pub cpu_load: Option<f64>,
        pub memory_available_mb: Option<i64>,
        pub active_tasks: Option<i32>,
        pub network_type: Option<String>,
        pub registered_at: DateTimeUtc,
        pub is_online: bool,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::clusters::Entity",
            from = "Column::ClusterId",
            to = "super::clusters::Column::Id"
        )]
        Cluster,
    }

    impl Related<super::clusters::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Cluster.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

/// ── users ──────────────────────────────────────────────────────────────────
pub mod users {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "users")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        #[sea_orm(unique)]
        pub email: String,
        pub password_hash: String,
        /// "super_admin" or "admin"
        pub role: String,
        pub created_at: DateTimeUtc,
        pub last_login: Option<DateTimeUtc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// ── plugins ────────────────────────────────────────────────────────────────
pub mod plugins {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, serde::Serialize)]
    #[sea_orm(table_name = "plugins")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub name: String,
        pub version: String,
        pub description: Option<String>,
        pub checksum: Option<String>,
        pub uploaded_by: Option<String>,
        pub uploaded_at: DateTimeUtc,
        pub approved: bool,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
