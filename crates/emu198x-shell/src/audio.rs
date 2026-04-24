//! Shared host-side audio conversion helpers.

/// Converts interleaved machine audio into interleaved host-output
/// audio, preserving channel positions when the source and output
/// channel counts match.
///
/// The conversion is intentionally simple: linear interpolation for
/// sample-rate conversion, channel copy for matching channel counts,
/// source-mono duplication for multi-channel output, and average
/// downmixing only when the host has fewer channels than the source.
#[must_use]
pub fn convert_audio_packet(
    samples: &[f32],
    source_rate: u32,
    source_channels: u8,
    output_rate: u32,
    output_channels: u16,
) -> Vec<f32> {
    let source_channel_count = usize::from(source_channels);
    let output_channel_count = usize::from(output_channels);
    if samples.is_empty()
        || source_rate == 0
        || output_rate == 0
        || source_channel_count == 0
        || output_channel_count == 0
    {
        return Vec::new();
    }

    let source_frames = samples.len() / source_channel_count;
    if source_frames == 0 {
        return Vec::new();
    }

    let output_frames = if source_rate == output_rate {
        source_frames
    } else {
        usize::try_from(
            (source_frames as u64 * u64::from(output_rate)).div_ceil(u64::from(source_rate)),
        )
        .unwrap_or(usize::MAX)
    };
    let mut converted = Vec::with_capacity(output_frames.saturating_mul(output_channel_count));

    if source_rate == output_rate {
        for frame in 0..source_frames {
            push_converted_frame(
                &mut converted,
                samples,
                frame,
                0.0,
                source_frames,
                source_channel_count,
                output_channel_count,
            );
        }
        return converted;
    }

    let step = f64::from(source_rate) / f64::from(output_rate);
    for frame in 0..output_frames {
        let position = frame as f64 * step;
        let index = position.floor() as usize;
        let frac = (position - index as f64) as f32;
        push_converted_frame(
            &mut converted,
            samples,
            index,
            frac,
            source_frames,
            source_channel_count,
            output_channel_count,
        );
    }

    converted
}

fn push_converted_frame(
    output: &mut Vec<f32>,
    samples: &[f32],
    frame_index: usize,
    frac: f32,
    source_frames: usize,
    source_channels: usize,
    output_channels: usize,
) {
    for output_channel in 0..output_channels {
        output.push(convert_channel(
            samples,
            frame_index,
            frac,
            source_frames,
            source_channels,
            output_channels,
            output_channel,
        ));
    }
}

fn convert_channel(
    samples: &[f32],
    frame_index: usize,
    frac: f32,
    source_frames: usize,
    source_channels: usize,
    output_channels: usize,
    output_channel: usize,
) -> f32 {
    if output_channels < source_channels {
        let mut sum = 0.0f32;
        for source_channel in 0..source_channels {
            sum += interpolate_channel(
                samples,
                frame_index,
                frac,
                source_frames,
                source_channels,
                source_channel,
            );
        }
        return sum / source_channels as f32;
    }

    if output_channels == source_channels
        || source_channels == 1
        || output_channel < source_channels
    {
        let source_channel = output_channel.min(source_channels.saturating_sub(1));
        return interpolate_channel(
            samples,
            frame_index,
            frac,
            source_frames,
            source_channels,
            source_channel,
        );
    }

    0.0
}

fn interpolate_channel(
    samples: &[f32],
    frame_index: usize,
    frac: f32,
    source_frames: usize,
    source_channels: usize,
    source_channel: usize,
) -> f32 {
    let last = source_frames.saturating_sub(1);
    let a = samples[(frame_index.min(last) * source_channels) + source_channel];
    let b = samples[((frame_index + 1).min(last) * source_channels) + source_channel];
    a + (b - a) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_stereo_when_output_is_stereo() {
        let converted = convert_audio_packet(&[0.25, -0.5, 0.75, -1.0], 48_000, 2, 48_000, 2);

        assert_eq!(converted, vec![0.25, -0.5, 0.75, -1.0]);
    }

    #[test]
    fn duplicates_mono_to_stereo() {
        let converted = convert_audio_packet(&[0.25, -0.5], 44_100, 1, 44_100, 2);

        assert_eq!(converted, vec![0.25, 0.25, -0.5, -0.5]);
    }

    #[test]
    fn fills_extra_output_channels_with_silence() {
        let converted = convert_audio_packet(&[0.25, -0.5], 48_000, 2, 48_000, 4);

        assert_eq!(converted, vec![0.25, -0.5, 0.0, 0.0]);
    }

    #[test]
    fn downmixes_stereo_to_mono() {
        let converted = convert_audio_packet(&[1.0, -0.5, 0.5, 0.25], 48_000, 2, 48_000, 1);

        assert_eq!(converted, vec![0.25, 0.375]);
    }

    #[test]
    fn resamples_each_channel_independently() {
        let converted = convert_audio_packet(&[0.0, 1.0, 1.0, 3.0], 2, 2, 4, 2);

        assert_eq!(converted, vec![0.0, 1.0, 0.5, 2.0, 1.0, 3.0, 1.0, 3.0]);
    }

    #[test]
    fn ignores_incomplete_source_frame() {
        let converted = convert_audio_packet(&[1.0, 2.0, 3.0], 48_000, 2, 48_000, 2);

        assert_eq!(converted, vec![1.0, 2.0]);
    }
}
