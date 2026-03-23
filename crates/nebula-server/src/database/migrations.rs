//! Versioned database migrations using `sea-orm-migration`.
//!
//! Run via `Migrator::up(&db, None).await` at server startup.

use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(M001CreateTables)]
    }
}

// ── M001: Initial schema ────────────────────────────────────────────────────

#[derive(DeriveMigrationName)]
pub struct M001CreateTables;

/// Identifiers shared by the migration DDL.  Using an enum per table keeps
/// things type-safe and avoids stringly-typed column names.
#[derive(DeriveIden)]
enum Clusters {
    Table,
    Id,
    Name,
    CreatedAt,
    MaxNodes,
    AuthTokenHash,
}

#[derive(DeriveIden)]
enum Nodes {
    Table,
    Id,
    ClusterId,
    Role,
    LastHeartbeat,
    BatteryLevel,
    CpuLoad,
    MemoryAvailableMb,
    ActiveTasks,
    NetworkType,
    RegisteredAt,
    IsOnline,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Email,
    PasswordHash,
    Role,
    CreatedAt,
    LastLogin,
}

#[derive(DeriveIden)]
enum Plugins {
    Table,
    Id,
    Name,
    Version,
    Description,
    Checksum,
    UploadedBy,
    UploadedAt,
    Approved,
}

#[async_trait::async_trait]
impl MigrationTrait for M001CreateTables {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── clusters ────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Clusters::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Clusters::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Clusters::Name).string().not_null())
                    .col(
                        ColumnDef::new(Clusters::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Clusters::MaxNodes)
                            .integer()
                            .not_null()
                            .default(1024),
                    )
                    .col(ColumnDef::new(Clusters::AuthTokenHash).string())
                    .to_owned(),
            )
            .await?;

        // ── nodes ───────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Nodes::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Nodes::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Nodes::ClusterId).string().not_null())
                    .col(ColumnDef::new(Nodes::Role).string().not_null())
                    .col(
                        ColumnDef::new(Nodes::LastHeartbeat)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Nodes::BatteryLevel).integer())
                    .col(ColumnDef::new(Nodes::CpuLoad).double())
                    .col(ColumnDef::new(Nodes::MemoryAvailableMb).big_integer())
                    .col(ColumnDef::new(Nodes::ActiveTasks).integer())
                    .col(ColumnDef::new(Nodes::NetworkType).string())
                    .col(
                        ColumnDef::new(Nodes::RegisteredAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Nodes::IsOnline)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_nodes_cluster")
                            .from(Nodes::Table, Nodes::ClusterId)
                            .to(Clusters::Table, Clusters::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // ── users ───────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Users::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Users::Email).string().not_null().unique_key())
                    .col(ColumnDef::new(Users::PasswordHash).string().not_null())
                    .col(ColumnDef::new(Users::Role).string().not_null())
                    .col(
                        ColumnDef::new(Users::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Users::LastLogin).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await?;

        // ── plugins ─────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Plugins::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Plugins::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Plugins::Name).string().not_null())
                    .col(ColumnDef::new(Plugins::Version).string().not_null())
                    .col(ColumnDef::new(Plugins::Description).text())
                    .col(ColumnDef::new(Plugins::Checksum).string())
                    .col(ColumnDef::new(Plugins::UploadedBy).string())
                    .col(
                        ColumnDef::new(Plugins::UploadedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Plugins::Approved)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        // ── indexes ─────────────────────────────────────────────────────
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_nodes_cluster_id")
                    .table(Nodes::Table)
                    .col(Nodes::ClusterId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_nodes_is_online")
                    .table(Nodes::Table)
                    .col(Nodes::IsOnline)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_plugins_name_version")
                    .table(Plugins::Table)
                    .col(Plugins::Name)
                    .col(Plugins::Version)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Plugins::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Nodes::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Clusters::Table).to_owned())
            .await?;
        Ok(())
    }
}
