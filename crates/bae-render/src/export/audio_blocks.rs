//! Mix once per requested audio block; deliver the samples in video timestamp order.
use super::{ExportJob, RenderError, media_error};
use bae_media::{audio::time_to_sample, mix::AudioMix};
use bae_model::Time;

pub(super) struct ExportAudio {
    mix: AudioMix,
    times: Vec<Time>,
    frame_duration: Time,
    block_samples: i64,
    first: i64,
    pcm: Vec<[f32; 2]>,
}

impl ExportAudio {
    pub(super) const RATE: u32 = 48_000;

    pub(super) fn new(job: &ExportJob<'_>) -> Self {
        // Bound working memory for exceptionally long user-entered durations.
        const MAX_SAMPLES: i64 = (32 << 20) / 8;
        let duration = if job.settings.audio_block > Time::ZERO {
            job.settings.audio_block
        } else {
            job.rate.frame_duration()
        };
        Self {
            mix: AudioMix::from_stage(job.stage, job.comp),
            times: job.times.iter().map(|&(_, t)| t).collect(),
            frame_duration: job.rate.frame_duration(),
            block_samples: time_to_sample(duration, Self::RATE).clamp(1, MAX_SAMPLES),
            first: 0,
            pcm: Vec::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.mix.is_empty()
    }

    pub(super) fn frame(
        &mut self,
        index: usize,
        mut write: impl FnMut(&[[f32; 2]]) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let time = self.times[index];
        let mut offset = time_to_sample(time, Self::RATE);
        let end = time_to_sample(time + self.frame_duration, Self::RATE);
        while offset < end {
            if !(self.first..self.first + self.pcm.len() as i64).contains(&offset) {
                // Stop mixing at a cut or at the end of the selected work area.
                let limit = offset + self.block_samples;
                let mut until = time + self.frame_duration;
                for &next in &self.times[index + 1..] {
                    if time_to_sample(until, Self::RATE) >= limit || next != until {
                        break;
                    }
                    until += self.frame_duration;
                }
                let count = time_to_sample(until, Self::RATE).min(limit) - offset;
                self.pcm.resize(count as usize, [0.0; 2]);
                self.mix
                    .render(offset, Self::RATE, &mut self.pcm)
                    .map_err(media_error)?;
                self.first = offset;
            }
            let start = (offset - self.first) as usize;
            let count = (self.pcm.len() - start).min((end - offset) as usize);
            write(&self.pcm[start..start + count])?;
            offset += count as i64;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::FrameRate;

    #[test]
    fn fractional_frames_share_blocks_and_stop_at_a_cut() {
        let rate = FrameRate::from_fps(29.97);
        let d = rate.frame_duration();
        let times = vec![Time::ZERO, d, d + d, Time::ONE_SECOND];
        let mut audio = ExportAudio {
            mix: AudioMix::default(),
            times,
            frame_duration: d,
            block_samples: 3000,
            first: 0,
            pcm: Vec::new(),
        };
        let mut written = 0;
        for i in 0..4 {
            audio
                .frame(i, |samples| {
                    written += samples.len();
                    Ok(())
                })
                .unwrap();
            if i == 0 {
                assert_eq!(audio.pcm.len(), 3000);
            }
            if i == 2 {
                assert_eq!(
                    audio.first + audio.pcm.len() as i64,
                    time_to_sample(d + d + d, ExportAudio::RATE)
                );
            }
        }
        let last =
            time_to_sample(Time::ONE_SECOND + d, ExportAudio::RATE) - i64::from(ExportAudio::RATE);
        assert_eq!(
            written as i64,
            time_to_sample(d + d + d, ExportAudio::RATE) + last
        );
    }
}
