use std::{
    fs::File,
    io::BufWriter,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use dashmap::DashMap;
use hound::{SampleFormat, WavSpec, WavWriter};
use poise::serenity_prelude as serenity;
use serenity::async_trait;
use songbird::{
    CoreEvent, Event, EventContext, EventHandler as VoiceEventHandler, Songbird,
    model::{
        id::UserId,
        payload::{ClientDisconnect, Speaking},
    },
};

/// ─────────────────────────────────────────────────────────────────────────────
/// 各ユーザーの PCM バッファ & SSRC マップを共有する内部構造体
/// ─────────────────────────────────────────────────────────────────────────────
struct InnerReceiver {
    last_tick_was_empty: AtomicBool,
    known_ssrcs: DashMap<u32, UserId>,
    pcm_buf: DashMap<UserId, Vec<i16>>,
}

#[derive(Clone)]
pub struct Receiver {
    inner: Arc<InnerReceiver>,
}

impl Receiver {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(InnerReceiver {
                last_tick_was_empty: AtomicBool::default(),
                known_ssrcs: DashMap::new(),
                pcm_buf: DashMap::new(),
            }),
        }
    }
}

/// ─────────────────────────────────────────────────────────────────────────────
/// Discord から受け取る各種イベントを処理
/// ─────────────────────────────────────────────────────────────────────────────
#[async_trait]
impl VoiceEventHandler for Receiver {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        use EventContext as Ctx;

        match ctx {
            // ── ユーザー ↔ SSRC のひも付け ────────────────────────────────
            Ctx::SpeakingStateUpdate(Speaking { ssrc, user_id, .. }) => {
                if let Some(uid) = user_id {
                    self.inner.known_ssrcs.insert(*ssrc, *uid);
                }
            }

            // ── 20 ms ごとの音声パケット (decoded_voice) を蓄積 ─────────
            Ctx::VoiceTick(tick) => {
                for (ssrc, data) in &tick.speaking {
                    if let (Some(decoded), Some(uid)) = (
                        data.decoded_voice.as_ref(),
                        self.inner.known_ssrcs.get(ssrc).map(|v| *v),
                    ) {
                        self.inner
                            .pcm_buf
                            .entry(uid)
                            .or_default()
                            .extend_from_slice(decoded);
                    }
                }

                let speaking = tick.speaking.len();
                if speaking == 0 {
                    self.inner.last_tick_was_empty.store(true, Ordering::SeqCst);
                } else {
                    self.inner
                        .last_tick_was_empty
                        .store(false, Ordering::SeqCst);
                }
            }

            // ── VC から退出したら WAV へ書き出し ───────────────────────
            Ctx::ClientDisconnect(ClientDisconnect { user_id, .. }) => {
                // `user_id` はすでに `UserId`
                if let Some((_k, pcm)) = self.inner.pcm_buf.remove(&user_id) {
                    let filename = format!("{user_id}.wav");
                    if let Err(e) = write_wav(&filename, &pcm) {
                        eprintln!("❌ Failed to write WAV: {e}");
                    } else {
                        println!("💾 Saved → {filename}  ({} samples)", pcm.len());
                    }
                }
            }
            _ => {}
        }

        None
    }
}

/// ─────────────────────────────────────────────────────────────────────────────
/// PCM (48 kHz/16-bit/mono) → WAV へ保存
/// ─────────────────────────────────────────────────────────────────────────────
fn write_wav(path: &str, samples: &[i16]) -> Result<(), Box<dyn std::error::Error>> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let writer = BufWriter::new(File::create(path)?);
    let mut wav = WavWriter::new(writer, spec)?;
    for &s in samples {
        wav.write_sample(s)?;
    }
    wav.finalize()?;
    Ok(())
}

/// ─────────────────────────────────────────────────────────────────────────────
/// ハンドラを Songbird に登録するヘルパ
/// ─────────────────────────────────────────────────────────────────────────────
pub async fn receive(manager: Arc<Songbird>, guild_id: serenity::GuildId) {
    let handler_lock = manager.get_or_insert(guild_id);
    let mut handler = handler_lock.lock().await;
    let evt_receiver = Receiver::new();

    handler.add_global_event(CoreEvent::SpeakingStateUpdate.into(), evt_receiver.clone());
    handler.add_global_event(CoreEvent::RtpPacket.into(), evt_receiver.clone());
    handler.add_global_event(CoreEvent::RtcpPacket.into(), evt_receiver.clone());
    handler.add_global_event(CoreEvent::ClientDisconnect.into(), evt_receiver.clone());
    handler.add_global_event(CoreEvent::VoiceTick.into(), evt_receiver);
}
