use songbird::tracks::TrackHandle;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::util::queue::MusicQueue;

pub struct Data {
    pub music: Arc<Mutex<MusicQueue>>,
    pub playing: Arc<Mutex<Option<TrackHandle>>>,
}
