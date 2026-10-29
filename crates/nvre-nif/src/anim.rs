//! The animation sequences.

/// A text key marking an event at a point in time (NiTextKeyExtraData).
#[derive(Debug, Clone, PartialEq)]
pub struct TextKey {
    pub time: f32,
    pub value: String,
}

/// A linear vector keyframe for position tracks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TranslationKey {
    pub time: f32,
    pub translation: [f32; 3],
}

/// Sample a translation track at time `t` with linear interpolation.
pub fn sample_translation_track(keys: &[TranslationKey], t: f32) -> Option<[f32; 3]> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 || t <= keys[0].time {
        return Some(keys[0].translation);
    }
    if t >= keys.last().unwrap().time {
        return Some(keys.last().unwrap().translation);
    }

    // Find the bounding segment
    for window in keys.windows(2) {
        let (k0, k1) = (&window[0], &window[1]);
        if t >= k0.time && t <= k1.time {
            let duration = k1.time - k0.time;
            let factor = if duration > 0.0 {
                (t - k0.time) / duration
            } else {
                0.0
            };
            return Some([
                k0.translation[0] + (k1.translation[0] - k0.translation[0]) * factor,
                k0.translation[1] + (k1.translation[1] - k0.translation[1]) * factor,
                k0.translation[2] + (k1.translation[2] - k0.translation[2]) * factor,
            ]);
        }
    }
    Some(keys.last().unwrap().translation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s11_2_text_keys_representation() {
        let keys = vec![
            TextKey { time: 0.0, value: "start".into() },
            TextKey { time: 0.5, value: ":hit".into() },
            TextKey { time: 1.0, value: "end".into() },
        ];
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[1].time, 0.5);
        assert_eq!(keys[1].value, ":hit");
    }

    #[test]
    fn s11_3_animation_sample_translation_track_interpolation() {
        let track = vec![
            TranslationKey { time: 0.0, translation: [0.0, 0.0, 0.0] },
            TranslationKey { time: 1.0, translation: [10.0, 20.0, -30.0] },
            TranslationKey { time: 2.0, translation: [20.0, 20.0, 0.0] },
        ];

        // Before start -> clamps to first
        assert_eq!(sample_translation_track(&track, -0.5), Some([0.0, 0.0, 0.0]));
        // Midpoint of segment 0..1 (t = 0.5)
        let mid = sample_translation_track(&track, 0.5).unwrap();
        assert!((mid[0] - 5.0).abs() < 1e-5);
        assert!((mid[1] - 10.0).abs() < 1e-5);
        assert!((mid[2] - (-15.0)).abs() < 1e-5);

        // Midpoint of segment 1..2 (t = 1.5)
        let mid2 = sample_translation_track(&track, 1.5).unwrap();
        assert!((mid2[0] - 15.0).abs() < 1e-5);
        assert!((mid2[1] - 20.0).abs() < 1e-5);
        assert!((mid2[2] - (-15.0)).abs() < 1e-5);

        // After end -> clamps to last
        assert_eq!(sample_translation_track(&track, 3.0), Some([20.0, 20.0, 0.0]));
    }
}
