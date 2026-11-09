//! 本地歌单同步引擎的 FRB 暴露面。
//!
//! [`crate::playlist_core`]（Loro CRDT 权威源）之上的薄壳：镜像出 FRB 友好的
//! 结构体，并提供落盘 / 快照导入导出接口。SQLite 物化缓存、Gist 同步编排均在
//! Dart 侧（见 `lib/src/data/sync/playlist_sync_controller.dart`）。
//!
//! 所有写操作与 `import` 返回**本次实际受影响的歌单 id 列表**，Dart 侧只重写
//! SQLite 中对应歌单的行。

use std::sync::Mutex;

use crate::playlist_core::{PlaylistCore, TrackEntry, LIKED_PLAYLIST_ID};

/// 内置「喜欢」歌单固定 id（Dart 侧同名常量须一致）。
pub fn liked_playlist_id() -> String {
    LIKED_PLAYLIST_ID.to_string()
}

/// 歌单摘要（物化用）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PlaylistInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    /// `user` | `liked` | `plugin`（插件歌单收藏）。
    pub kind: String,
    /// 创建时间（epoch 秒）。
    pub created_at: i64,
    /// 展示顺序（0 起）。
    pub position: u32,
    pub track_count: u32,
    /// 插件歌单收藏元数据；普通本地歌单为 `None`。
    pub source_plugin_id: Option<String>,
    pub source_playlist_id: Option<String>,
    pub source_label: Option<String>,
    pub cover_url: Option<String>,
    /// 收藏时间（epoch 秒；普通歌单回退为创建时间）。
    pub favorited_at: i64,
    /// 最近一次成功刷新完整快照的时间（epoch 毫秒）。
    pub refreshed_at: i64,
}

/// 歌单内曲目（跨插件引用，字段约定同 Dart `LikedTrack`）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PlaylistTrack {
    pub plugin_id: String,
    pub track_id: String,
    pub provider: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub cover: Option<String>,
    /// 时长（秒）。
    pub duration: Option<i64>,
    /// 加入时间（epoch 秒）。
    pub added_at: i64,
}

impl From<TrackEntry> for PlaylistTrack {
    fn from(t: TrackEntry) -> Self {
        PlaylistTrack {
            plugin_id: t.plugin_id,
            track_id: t.track_id,
            provider: t.provider,
            title: t.title,
            artists: t.artists,
            album: t.album,
            cover: t.cover,
            duration: t.duration,
            added_at: t.added_at,
        }
    }
}

impl From<&PlaylistTrack> for TrackEntry {
    fn from(t: &PlaylistTrack) -> Self {
        TrackEntry {
            plugin_id: t.plugin_id.clone(),
            track_id: t.track_id.clone(),
            provider: t.provider.clone(),
            title: t.title.clone(),
            artists: t.artists.clone(),
            album: t.album.clone(),
            cover: t.cover.clone(),
            duration: t.duration,
            added_at: t.added_at,
        }
    }
}

/// 本地歌单权威源（内部 LoroDoc）。FRB opaque，Dart 侧全 app 单例持有。
pub struct PlaylistSyncEngine {
    core: Mutex<PlaylistCore>,
}

impl PlaylistSyncEngine {
    /// 从磁盘快照打开；文件不存在或损坏视为空文档（设计稿 §7），不报错。
    pub fn open(path: String) -> PlaylistSyncEngine {
        let core = match std::fs::read(&path) {
            Ok(bytes) => PlaylistCore::from_snapshot(&bytes).unwrap_or_default(),
            Err(_) => PlaylistCore::new(),
        };
        PlaylistSyncEngine {
            core: Mutex::new(core),
        }
    }

    /// 快照落盘（临时文件 + rename，避免半截文件）。
    pub fn save_to_disk(&self, path: String) -> Result<(), String> {
        let bytes = self.core.lock().unwrap().export_snapshot();
        if let Some(dir) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = format!("{path}.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 替换内存文档为磁盘快照（一般仅 open 后无需再调；损坏视为空文档）。
    pub fn load_from_disk(&self, path: String) {
        let core = match std::fs::read(&path) {
            Ok(bytes) => PlaylistCore::from_snapshot(&bytes).unwrap_or_default(),
            Err(_) => PlaylistCore::new(),
        };
        *self.core.lock().unwrap() = core;
    }

    // ───────────────────────────── 读 ─────────────────────────────

    pub fn playlists(&self) -> Vec<PlaylistInfo> {
        self.core
            .lock()
            .unwrap()
            .playlists()
            .into_iter()
            .map(|p| PlaylistInfo {
                id: p.id,
                title: p.title,
                description: p.description,
                kind: p.kind,
                created_at: p.created_at,
                position: p.position,
                track_count: p.track_count,
                source_plugin_id: p.source_plugin_id,
                source_playlist_id: p.source_playlist_id,
                source_label: p.source_label,
                cover_url: p.cover_url,
                favorited_at: p.favorited_at,
                refreshed_at: p.refreshed_at,
            })
            .collect()
    }

    pub fn playlist_tracks(&self, id: String) -> Vec<PlaylistTrack> {
        self.core
            .lock()
            .unwrap()
            .playlist_tracks(&id)
            .into_iter()
            .map(PlaylistTrack::from)
            .collect()
    }

    // ───────────────────────────── 写 ─────────────────────────────

    /// 新建用户歌单，返回新 id。
    pub fn create_playlist(&self, title: String) -> String {
        self.core.lock().unwrap().create_playlist(&title)
    }

    /// 确保内置「喜欢」歌单存在（幂等）。返回是否本次新建。
    pub fn ensure_liked_playlist(&self, title: String) -> bool {
        self.core.lock().unwrap().ensure_liked_playlist(&title)
    }

    /// 收藏插件歌单：记录来源、封面、简介与曲目快照，返回本地歌单 id。
    /// 重复收藏幂等（元数据更新、完整曲目快照替换）。
    pub fn favorite_plugin_playlist(
        &self,
        plugin_id: String,
        playlist_id: String,
        source_label: String,
        title: String,
        description: String,
        cover_url: String,
        tracks: Vec<PlaylistTrack>,
    ) -> String {
        let tracks: Vec<TrackEntry> = tracks.iter().map(TrackEntry::from).collect();
        self.core.lock().unwrap().favorite_plugin_playlist(
            &plugin_id,
            &playlist_id,
            &source_label,
            &title,
            &description,
            &cover_url,
            &tracks,
        )
    }

    pub fn rename_playlist(&self, id: String, title: String, description: String) -> Vec<String> {
        self.core
            .lock()
            .unwrap()
            .rename_playlist(&id, &title, &description)
    }

    pub fn delete_playlist(&self, id: String) -> Vec<String> {
        self.core.lock().unwrap().delete_playlist(&id)
    }

    pub fn add_track(&self, playlist_id: String, track: PlaylistTrack) -> Vec<String> {
        self.core
            .lock()
            .unwrap()
            .add_track(&playlist_id, &TrackEntry::from(&track))
    }

    pub fn remove_track(
        &self,
        playlist_id: String,
        plugin_id: String,
        track_id: String,
    ) -> Vec<String> {
        self.core
            .lock()
            .unwrap()
            .remove_track(&playlist_id, &plugin_id, &track_id)
    }

    pub fn move_track(&self, playlist_id: String, from: u32, to: u32) -> Vec<String> {
        self.core
            .lock()
            .unwrap()
            .move_track(&playlist_id, from as usize, to as usize)
    }

    pub fn reorder_playlists(&self, order: Vec<String>) -> Vec<String> {
        self.core.lock().unwrap().reorder_playlists(&order)
    }

    // ───────────────────────────── 同步 ─────────────────────────────

    /// 导出全量快照（推送 Gist 用）。
    pub fn export_snapshot(&self) -> Vec<u8> {
        self.core.lock().unwrap().export_snapshot()
    }

    /// 合并远端快照，返回受影响歌单 id。快照损坏返回 Err（上层记为同步失败）。
    pub fn import(&self, bytes: Vec<u8>) -> Result<Vec<String>, String> {
        self.core.lock().unwrap().import(&bytes)
    }
}
